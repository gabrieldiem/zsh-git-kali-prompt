use std::process::Command;

fn run_command(cmd: &str, args: &[&str]) -> Option<String> {
    Command::new(cmd)
        .args(args)
        .stderr(std::process::Stdio::null())
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn get_branch_info() -> (String, i32, i32) {
    let branch = run_command("git", &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_else(|| "main".to_string());

    if branch == "HEAD" {
        let short_head = run_command("git", &["rev-parse", "--short", "HEAD"])
            .unwrap_or_else(|| "unknown".to_string());
        return (format!(":{}", short_head), 0, 0);
    }

    let remote_name = run_command("git", &["config", &format!("branch.{}.remote", branch)])
        .unwrap_or_else(|| "origin".to_string());

    let merge_name = run_command("git", &["config", &format!("branch.{}.merge", branch)])
        .unwrap_or_else(|| format!("refs/heads/{}", branch));

    let remote_ref = if remote_name == "." {
        merge_name
    } else {
        let branch_part = merge_name.strip_prefix("refs/heads/").unwrap_or(&merge_name);
        format!("refs/remotes/{}/{}", remote_name, branch_part)
    };

    let rev_list_output = run_command("git", &["rev-list", "--left-right", &format!("{}...HEAD", remote_ref)])
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

    (branch, ahead, behind)
}

fn count_files(args: &[&str]) -> i32 {
    run_command("git", args)
        .map(|output| output.lines().count() as i32)
        .unwrap_or(0)
}

fn main() {
    // Quick check if we're in a git repository
    if run_command("git", &["rev-parse", "--is-inside-work-tree"]).is_none() {
        std::process::exit(1);
    }

    // Sequential execution for minimal binary size
    let (branch, ahead, behind) = get_branch_info();
    let staged = count_files(&["diff", "--cached", "--name-only"]);
    let conflicts = count_files(&["diff", "--name-only", "--diff-filter=U"]);
    let modified = count_files(&["diff", "--name-only", "--diff-filter=M"]);
    let untracked = count_files(&["ls-files", "--others", "--exclude-standard"]);
    let deleted = count_files(&["diff", "--name-only", "--diff-filter=D"]);

    println!("{} {} {} {} {} {} {} {}",
             branch, ahead, behind, staged, conflicts, modified, untracked, deleted);
}