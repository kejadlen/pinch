use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{Context, Result, bail};

/// Derive a bare repo path from a remote URL.
/// e.g. "https://github.com/user/repo" -> repos_dir/github.com/user/repo.git
pub fn repo_path(repos_dir: &Path, src: &str) -> PathBuf {
    let url = src
        .strip_prefix("https://")
        .or_else(|| src.strip_prefix("http://"))
        .unwrap_or(src)
        .trim_end_matches(".git");
    repos_dir.join(format!("{url}.git"))
}

/// Path to a worktree for a specific revision.
/// e.g. repos_dir/github.com/user/repo.git/worktrees/<rev_short>
pub fn worktree_path(repo_path: &Path, rev: &str) -> PathBuf {
    repo_path.join("pinch-worktrees").join(&rev[..12])
}

pub fn clone_or_fetch(repo_path: &Path, src: &str) -> Result<()> {
    if repo_path.join("HEAD").is_file() {
        let status = Command::new("git")
            .args(["fetch", "--quiet", "origin"])
            .current_dir(repo_path)
            .status()
            .context("failed to run git fetch")?;
        if !status.success() {
            bail!("git fetch failed for {}", src);
        }
    } else {
        std::fs::create_dir_all(repo_path)
            .with_context(|| format!("failed to create {}", repo_path.display()))?;
        let status = Command::new("git")
            .args(["clone", "--quiet", "--bare", src, "."])
            .current_dir(repo_path)
            .status()
            .context("failed to run git clone")?;
        if !status.success() {
            bail!("git clone failed for {}", src);
        }
    }
    Ok(())
}

pub fn resolve_ref(repo_path: &Path, git_ref: &str) -> Result<String> {
    // Try origin/<ref> first (branch)
    let output = Command::new("git")
        .args(["rev-parse", &format!("origin/{git_ref}")])
        .current_dir(repo_path)
        .output()
        .context("failed to run git rev-parse")?;

    if output.status.success() {
        return Ok(String::from_utf8_lossy(&output.stdout).trim().to_string());
    }

    // Fall back to treating it as a tag or raw ref
    let output = Command::new("git")
        .args(["rev-parse", git_ref])
        .current_dir(repo_path)
        .output()
        .context("failed to run git rev-parse")?;

    if !output.status.success() {
        bail!(
            "could not resolve ref '{}' in {}",
            git_ref,
            repo_path.display()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Ensure a worktree exists at the given rev and return its path.
pub fn ensure_worktree(repo_path: &Path, rev: &str) -> Result<PathBuf> {
    let wt_path = worktree_path(repo_path, rev);

    if wt_path.join(".git").exists() {
        // Worktree already exists, make sure it's at the right rev
        let status = Command::new("git")
            .args(["checkout", "--quiet", "--detach", rev])
            .current_dir(&wt_path)
            .status()
            .context("failed to run git checkout in worktree")?;
        if !status.success() {
            bail!("git checkout failed for {} in {}", rev, wt_path.display());
        }
    } else {
        std::fs::create_dir_all(wt_path.parent().unwrap())?;
        let status = Command::new("git")
            .args([
                "worktree",
                "add",
                "--quiet",
                "--detach",
                &wt_path.to_string_lossy(),
                rev,
            ])
            .current_dir(repo_path)
            .status()
            .context("failed to run git worktree add")?;
        if !status.success() {
            bail!(
                "git worktree add failed for {} in {}",
                rev,
                repo_path.display()
            );
        }
    }

    Ok(wt_path)
}
