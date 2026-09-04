use std::{path::Path, process::Command};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitContext {
    pub branch: String,
    pub dirty: bool,
}

pub fn detect(path: &Path) -> Option<GitContext> {
    let working_dir = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));

    let branch = git_output(working_dir, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch.is_empty() || branch == "HEAD" {
        return None;
    }

    let dirty = git_output(working_dir, &["status", "--porcelain"])
        .is_some_and(|output| !output.trim().is_empty());

    Some(GitContext { branch, dirty })
}

fn git_output(working_dir: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(working_dir)
        .args(args)
        .output()
        .ok()?;

    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}
