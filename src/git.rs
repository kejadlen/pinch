use std::path::{Path, PathBuf};
use std::process::Command;

use color_eyre::eyre::{Context, Result, bail};
use tracing::info;

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
        fs_err::create_dir_all(repo_path)?;
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
        fs_err::create_dir_all(wt_path.parent().unwrap())?;
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

/// Remove worktrees that aren't referenced by the lockfile.
pub fn prune_worktrees(repos_dir: &Path, lockfile: &crate::lockfile::Lockfile) -> Result<()> {
    use std::collections::{HashMap, HashSet};

    // Build a map: repo_path -> set of used rev prefixes
    let mut used: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    for plugin in lockfile.plugins.values() {
        let rp = repo_path(repos_dir, &plugin.src);
        used.entry(rp)
            .or_default()
            .insert(plugin.rev[..12].to_string());
    }

    // Walk all bare repos looking for pinch-worktrees dirs
    visit_worktree_dirs(repos_dir, &used)?;

    Ok(())
}

fn visit_worktree_dirs(
    dir: &Path,
    used: &std::collections::HashMap<PathBuf, std::collections::HashSet<String>>,
) -> Result<()> {
    let entries = match fs_err::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = entry?;
        let path = entry.path();

        if !path.is_dir() {
            continue;
        }

        if path.file_name().is_some_and(|n| n == "pinch-worktrees") {
            // This is a worktrees dir inside a bare repo
            let repo_path = path.parent().unwrap();
            let used_revs = used.get(repo_path);

            for wt_entry in fs_err::read_dir(&path)? {
                let wt_entry = wt_entry?;
                let wt_name = wt_entry.file_name();
                let wt_name_str = wt_name.to_string_lossy();

                let is_used = used_revs.is_some_and(|revs| revs.contains(wt_name_str.as_ref()));

                if !is_used {
                    let wt_path = wt_entry.path();
                    // Remove the git worktree properly
                    let _ = std::process::Command::new("git")
                        .args(["worktree", "remove", "--force", &wt_path.to_string_lossy()])
                        .current_dir(repo_path)
                        .status();
                    // If that fails, just remove the directory
                    if wt_path.exists() {
                        fs_err::remove_dir_all(&wt_path)?;
                    }
                    info!("pruned worktree: {}", wt_path.display());
                }
            }
        } else {
            visit_worktree_dirs(&path, used)?;
        }
    }

    Ok(())
}
