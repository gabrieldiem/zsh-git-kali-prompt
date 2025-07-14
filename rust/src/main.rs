use std::process::Command;
use std::sync::mpsc;
use std::thread;

#[derive(Debug, Default)]
struct GitStatus {
    branch: String,
    ahead: i32,
    behind: i32,
    staged: i32,
    conflicts: i32,
    modified: i32,
    untracked: i32,
    deleted: i32,
}

fn run_command(
    cmd: &str,
    args: &[&str],
) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
    let output = Command::new(cmd)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()?;

    if !output.status.success() {
        return Err(format!("Command failed: {} {}", cmd, args.join(" ")).into());
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_branch_info() -> Result<(String, i32, i32), Box<dyn std::error::Error + Send + Sync>> {
    let branch = run_command("git", &["rev-parse", "--abbrev-ref", "HEAD"])?;

    if branch == "HEAD" {
        let short_head = run_command("git", &["rev-parse", "--short", "HEAD"])?;
        return Ok((format!(":{}", short_head), 0, 0));
    }

    // Get remote tracking info
    let remote_name = run_command("git", &["config", &format!("branch.{}.remote", branch)])
        .unwrap_or_else(|_| "origin".to_string());

    let merge_name = run_command("git", &["config", &format!("branch.{}.merge", branch)])
        .unwrap_or_else(|_| format!("refs/heads/{}", branch));

    let remote_ref = if remote_name == "." {
        merge_name
    } else {
        let branch_part = merge_name
            .strip_prefix("refs/heads/")
            .unwrap_or(&merge_name);
        format!("refs/remotes/{}/{}", remote_name, branch_part)
    };

    // Get ahead/behind counts
    let rev_list_output = run_command(
        "git",
        &[
            "rev-list",
            "--left-right",
            &format!("{}...HEAD", remote_ref),
        ],
    )
    .unwrap_or_default();

    let mut ahead = 0;
    let mut behind = 0;

    for line in rev_list_output.lines() {
        if line.starts_with('>') {
            ahead += 1;
        } else if line.starts_with('<') {
            behind += 1;
        }
    }

    Ok((branch, ahead, behind))
}

fn count_files(args: &[&str]) -> Result<i32, Box<dyn std::error::Error + Send + Sync>> {
    let output = run_command("git", args)?;
    Ok(output.lines().count() as i32)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Quick check if we're in a git repository
    if run_command("git", &["rev-parse", "--is-inside-work-tree"]).is_err() {
        std::process::exit(1);
    }

    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();

    // Branch info thread
    let tx1 = tx.clone();
    handles.push(thread::spawn(move || {
        let result = get_branch_info();
        tx1.send(("branch", result)).unwrap();
    }));

    // Staged files thread
    let tx2 = tx.clone();
    handles.push(thread::spawn(move || {
        let result = count_files(&["diff", "--cached", "--name-only"]);
        tx2.send(("staged", result)).unwrap();
    }));

    // Conflicts thread
    let tx3 = tx.clone();
    handles.push(thread::spawn(move || {
        let result = count_files(&["diff", "--name-only", "--diff-filter=U"]);
        tx3.send(("conflicts", result)).unwrap();
    }));

    // Modified files thread
    let tx4 = tx.clone();
    handles.push(thread::spawn(move || {
        let result = count_files(&["diff", "--name-only", "--diff-filter=M"]);
        tx4.send(("modified", result)).unwrap();
    }));

    // Untracked files thread
    let tx5 = tx.clone();
    handles.push(thread::spawn(move || {
        let result = count_files(&["ls-files", "--others", "--exclude-standard"]);
        tx5.send(("untracked", result)).unwrap();
    }));

    // Deleted files thread
    let tx6 = tx.clone();
    handles.push(thread::spawn(move || {
        let result = count_files(&["diff", "--name-only", "--diff-filter=D"]);
        tx6.send(("deleted", result)).unwrap();
    }));

    // Drop the original sender so the channel can close
    drop(tx);

    let mut status = GitStatus::default();

    // Collect results from all threads
    for _ in 0..6 {
        match rx.recv() {
            Ok(("branch", Ok((branch, ahead, behind)))) => {
                status.branch = branch;
                status.ahead = ahead;
                status.behind = behind;
            }
            Ok(("staged", Ok(count))) => status.staged = count,
            Ok(("conflicts", Ok(count))) => status.conflicts = count,
            Ok(("modified", Ok(count))) => status.modified = count,
            Ok(("untracked", Ok(count))) => status.untracked = count,
            Ok(("deleted", Ok(count))) => status.deleted = count,
            Ok((_, Err(_))) => {} // Ignore errors for individual operations
            Err(_) => break,
        }
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().unwrap();
    }

    println!(
        "{} {} {} {} {} {} {} {}",
        status.branch,
        status.ahead,
        status.behind,
        status.staged,
        status.conflicts,
        status.modified,
        status.untracked,
        status.deleted
    );

    Ok(())
}
