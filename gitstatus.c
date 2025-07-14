#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <pthread.h>
#include <unistd.h>

#define MAX_THREADS 8
#define BUFFER_SIZE 1024

// Structure to pass data to threads
typedef struct {
    const char* command;
    char result[BUFFER_SIZE];
    int status;
    int thread_id;
} thread_data_t;

// Thread-safe command execution
int run_command(const char* cmd, char* result, int max_len) {
    FILE *fp = popen(cmd, "r");
    if (fp == NULL) {
        return -1;
    }
    
    if (fgets(result, max_len, fp) == NULL) {
        pclose(fp);
        return -1;
    }
    
    pclose(fp);
    // Remove trailing newline
    result[strcspn(result, "\n")] = 0;
    return 0;
}

// Thread function for executing commands
void* command_thread(void* arg) {
    thread_data_t* data = (thread_data_t*)arg;
    data->status = run_command(data->command, data->result, BUFFER_SIZE);
    return NULL;
}

// Thread function for ahead/behind calculation
void* ahead_behind_thread(void* arg) {
    thread_data_t* data = (thread_data_t*)arg;
    
    char branch[256];
    strcpy(branch, data->result); // Branch name passed in result field
    
    char remote_name[256] = {0}, merge_name[256] = {0}, remote_ref[256] = {0};
    char temp_result[BUFFER_SIZE];
    int ahead = 0, behind = 0;
    
    // Check if on a specific branch or detached HEAD
    if (strcmp(branch, "HEAD") == 0) {
        if (run_command("git rev-parse --short HEAD", temp_result, sizeof(temp_result)) == 0) {
            snprintf(data->result, BUFFER_SIZE, ":%s", temp_result);
        }
        // For detached HEAD, ahead/behind are 0
        snprintf(data->result + strlen(data->result), BUFFER_SIZE - strlen(data->result), " 0 0");
    } else {
        // Get remote configuration
        char remote_cmd[512];
        snprintf(remote_cmd, sizeof(remote_cmd), "git config branch.%s.remote", branch);
        
        char merge_cmd[512];
        snprintf(merge_cmd, sizeof(merge_cmd), "git config branch.%s.merge", branch);
        
        if (run_command(remote_cmd, remote_name, sizeof(remote_name)) == 0 &&
            run_command(merge_cmd, merge_name, sizeof(merge_name)) == 0) {
            
            if (strcmp(remote_name, ".") == 0) {
                snprintf(remote_ref, sizeof(remote_ref), "%s", merge_name);
            } else {
                snprintf(remote_ref, sizeof(remote_ref), "refs/remotes/%s/%s",
                        remote_name, merge_name + 11);
            }
            
            char revlist_cmd[512];
            snprintf(revlist_cmd, sizeof(revlist_cmd), "git rev-list --left-right %s...HEAD 2>/dev/null", remote_ref);
            
            FILE *revlist_fp = popen(revlist_cmd, "r");
            if (revlist_fp != NULL) {
                while (fgets(temp_result, sizeof(temp_result), revlist_fp) != NULL) {
                    if (temp_result[0] == '>') {
                        ahead++;
                    } else {
                        behind++;
                    }
                }
                pclose(revlist_fp);
            }
        }
        
        snprintf(data->result, BUFFER_SIZE, "%s %d %d", branch, ahead, behind);
    }
    
    data->status = 0;
    return NULL;
}

int main() {
    char result[BUFFER_SIZE];
    
    // Check if inside a git repository
    if (run_command("git rev-parse --is-inside-work-tree 2>/dev/null", result, sizeof(result)) != 0 ||
        strcmp(result, "true") != 0) {
        return 1;
    }
    
    // Get current branch name first (needed for ahead/behind calculation)
    if (run_command("git rev-parse --abbrev-ref HEAD", result, sizeof(result)) != 0) {
        return 1;
    }
    
    char branch[256];
    strcpy(branch, result);
    
    // Prepare thread data for concurrent Git commands
    pthread_t threads[MAX_THREADS];
    thread_data_t thread_data[MAX_THREADS];
    
    // Commands that can run concurrently
    const char* commands[] = {
        "git diff --cached --numstat | wc -l",                           // staged
        "git --no-pager diff --name-only --diff-filter=U | wc -l",      // conflicts
        "git --no-pager diff --name-only --diff-filter=M | wc -l",      // modified
        "git ls-files --others --exclude-standard | wc -l",             // untracked
        "git --no-pager diff --name-only --diff-filter=D | wc -l"       // deleted
    };
    
    int num_commands = sizeof(commands) / sizeof(commands[0]);
    
    // Launch threads for Git status commands
    for (int i = 0; i < num_commands; i++) {
        thread_data[i].command = commands[i];
        thread_data[i].thread_id = i;
        pthread_create(&threads[i], NULL, command_thread, &thread_data[i]);
    }
    
    // Launch thread for ahead/behind calculation
    strcpy(thread_data[num_commands].result, branch); // Pass branch name
    thread_data[num_commands].thread_id = num_commands;
    pthread_create(&threads[num_commands], NULL, ahead_behind_thread, &thread_data[num_commands]);
    
    // Wait for all threads to complete
    for (int i = 0; i <= num_commands; i++) {
        pthread_join(threads[i], NULL);
    }
    
    // Check for errors
    for (int i = 0; i < num_commands; i++) {
        if (thread_data[i].status != 0) {
            fprintf(stderr, "Error executing command %d\n", i);
            return 1;
        }
    }
    
    if (thread_data[num_commands].status != 0) {
        fprintf(stderr, "Error calculating ahead/behind\n");
        return 1;
    }
    
    // Parse results
    int staged = atoi(thread_data[0].result);
    int conflicts = atoi(thread_data[1].result);
    int modified = atoi(thread_data[2].result);
    int untracked = atoi(thread_data[3].result);
    int deleted = atoi(thread_data[4].result);
    
    // Parse ahead/behind result
    char final_branch[256];
    int ahead, behind;
    sscanf(thread_data[num_commands].result, "%s %d %d", final_branch, &ahead, &behind);
    
    printf("%s %d %d %d %d %d %d %d\n", final_branch, ahead, behind, staged, conflicts, modified, untracked, deleted);
    
    return 0;
}