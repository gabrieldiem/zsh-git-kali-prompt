use std::process::Command;
use std::thread::spawn;

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
        return Ok((format!(":{short_head}"), 0, 0));
    }

    // Get remote tracking info
    let remote_name = run_command("git", &["config", &format!("branch.{branch}.remote")])
        .unwrap_or_else(|_| "origin".to_string());

    let merge_name = run_command("git", &["config", &format!("branch.{branch}.merge")])
        .unwrap_or_else(|_| format!("refs/heads/{branch}"));

    let remote_ref = if remote_name == "." {
        merge_name
    } else {
        let branch_part = merge_name
            .strip_prefix("refs/heads/")
            .unwrap_or(&merge_name);
        format!("refs/remotes/{remote_name}/{branch_part}")
    };

    // Get ahead/behind counts
    let rev_list_output = run_command(
        "git",
        &["rev-list", "--left-right", &format!("{remote_ref}...HEAD")],
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

    // Fork: spawn all threads
    let branch_handle = spawn(get_branch_info);
    let staged_handle = spawn(|| count_files(&["diff", "--cached", "--name-only"]));
    let conflicts_handle =
        spawn(|| count_files(&["diff", "--name-only", "--diff-filter=U"]));
    let modified_handle =
        spawn(|| count_files(&["diff", "--name-only", "--diff-filter=M"]));
    let untracked_handle =
        spawn(|| count_files(&["ls-files", "--others", "--exclude-standard"]));
    let deleted_handle = spawn(|| count_files(&["diff", "--name-only", "--diff-filter=D"]));

    // Join: collect all results
    let mut status = GitStatus::default();

    if let Ok((branch, ahead, behind)) = branch_handle
        .join()
        .unwrap_or_else(|_| Ok(("main".to_string(), 0, 0)))
    {
        status.branch = branch;
        status.ahead = ahead;
        status.behind = behind;
    }

    status.staged = staged_handle.join().unwrap_or_else(|_| Ok(0)).unwrap_or(0);
    status.conflicts = conflicts_handle
        .join()
        .unwrap_or_else(|_| Ok(0))
        .unwrap_or(0);
    status.modified = modified_handle
        .join()
        .unwrap_or_else(|_| Ok(0))
        .unwrap_or(0);
    status.untracked = untracked_handle
        .join()
        .unwrap_or_else(|_| Ok(0))
        .unwrap_or(0);
    status.deleted = deleted_handle.join().unwrap_or_else(|_| Ok(0)).unwrap_or(0);

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
