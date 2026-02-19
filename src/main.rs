#![deny(clippy::disallowed_methods)]

mod config;
mod git;
mod lockfile;
mod marketplace;
mod paths;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Result, WrapErr, bail};
use tracing::info;

#[derive(Parser)]
#[command(
    name = "pinch",
    about = "A plugin manager for Claude Code and pi",
    version = option_env!("PINCH_VERSION").unwrap_or("dev")
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Install plugins from the lockfile
    Install {
        /// Run update before installing
        #[arg(long)]
        update: bool,
    },
    /// Update the lockfile from config
    Update {
        /// Install after updating
        #[arg(long)]
        install: bool,
        /// Update only the named plugin(s)
        names: Vec<String>,
    },
}

enum Action {
    Update(Vec<String>),
    Install,
}

fn main() -> Result<()> {
    color_eyre::install()?;
    tracing_subscriber::fmt::init();

    let cli = Cli::parse();

    let actions = match cli.command {
        Command::Install { update } => {
            let mut actions = vec![];
            if update {
                actions.push(Action::Update(vec![]));
            }
            actions.push(Action::Install);
            actions
        }
        Command::Update { install, names } => {
            let mut actions = vec![Action::Update(names)];
            if install {
                actions.push(Action::Install);
            }
            actions
        }
    };

    for action in actions {
        match action {
            Action::Update(names) => do_update(&names)?,
            Action::Install => do_install()?,
        }
    }

    Ok(())
}

fn do_update(names: &[String]) -> Result<()> {
    let all_plugins = config::load().wrap_err("failed to load config")?;

    let (plugins, mut lockfile) = if names.is_empty() {
        (
            all_plugins,
            lockfile::Lockfile {
                plugins: Default::default(),
            },
        )
    } else {
        let known: std::collections::HashSet<&str> =
            all_plugins.iter().map(|p| p.name.as_str()).collect();
        let mut unknown: Vec<_> = names
            .iter()
            .filter(|n| !known.contains(n.as_str()))
            .collect();
        if !unknown.is_empty() {
            unknown.sort();
            bail!(
                "unknown plugin(s): {}",
                unknown
                    .iter()
                    .map(|s| s.as_str())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }

        let requested: std::collections::HashSet<&str> = names.iter().map(|s| s.as_str()).collect();
        let plugins = all_plugins
            .into_iter()
            .filter(|p| requested.contains(p.name.as_str()))
            .collect();
        let lockfile = lockfile::load()?;
        (plugins, lockfile)
    };

    let repos_dir = paths::repos_dir()?;

    for plugin in &plugins {
        info!("updating {}...", plugin.name);
        let repo_path = git::repo_path(&repos_dir, &plugin.src);
        git::clone_or_fetch(&repo_path, &plugin.src)?;
        let rev = git::resolve_ref(&repo_path, &plugin.version)?;
        info!("  {} -> {}", plugin.version, &rev[..12]);

        // Create worktree to read marketplace.json and discover skill path
        let wt_path = git::ensure_worktree(&repo_path, &rev)?;
        let source =
            marketplace::find_plugin_source(&wt_path, &plugin.name, plugin.manifest.as_deref())?;
        let path = source.trim_start_matches("./").to_string();
        info!("  found at {}", path);

        lockfile.plugins.insert(
            plugin.name.clone(),
            lockfile::LockedPlugin {
                src: plugin.src.clone(),
                path,
                rev,
            },
        );
    }

    lockfile::save(&lockfile).wrap_err("failed to save lockfile")?;
    info!("lockfile updated");
    Ok(())
}

fn do_install() -> Result<()> {
    let lockfile = lockfile::load().wrap_err("no lockfile found — run `pinch update` first")?;

    let repos_dir = paths::repos_dir()?;
    let skills_dir = paths::skills_dir()?;
    let commands_dir = paths::commands_dir()?;
    let cache_dir = paths::cache_dir();
    let mut installed_skills = std::collections::HashSet::new();
    let mut installed_commands = std::collections::HashSet::new();

    for (name, plugin) in &lockfile.plugins {
        let repo_path = git::repo_path(&repos_dir, &plugin.src);
        let wt_path = git::ensure_worktree(&repo_path, &plugin.rev)?;

        let plugin_root = wt_path.join(&plugin.path);
        if !plugin_root.is_dir() {
            bail!(
                "plugin '{}': path '{}' does not exist in repo at rev {}",
                name,
                plugin.path,
                &plugin.rev[..12]
            );
        }

        reject_unsupported(name, &plugin_root)?;

        let has_skills = symlink_entries(
            &plugin_root,
            "skills",
            &skills_dir,
            true,
            &mut installed_skills,
        )?;
        let has_commands = symlink_entries(
            &plugin_root,
            "commands",
            &commands_dir,
            false,
            &mut installed_commands,
        )?;

        if !has_skills && !has_commands {
            bail!(
                "plugin '{}': no skills/ or commands/ directory in '{}'",
                name,
                plugin.path
            );
        }
    }

    remove_stale_symlinks(&skills_dir, &cache_dir, &installed_skills, "skill")?;
    remove_stale_symlinks(&commands_dir, &cache_dir, &installed_commands, "command")?;

    git::prune(&repos_dir, &lockfile)?;

    info!("install complete");
    Ok(())
}

/// Bail if the plugin contains types we don't support yet.
fn reject_unsupported(name: &str, plugin_root: &std::path::Path) -> Result<()> {
    for unsupported in ["agents", "hooks"] {
        if plugin_root.join(unsupported).is_dir() {
            bail!(
                "plugin '{}': contains '{}/' — only skills and commands are supported",
                name,
                unsupported
            );
        }
    }
    for unsupported in [".mcp.json", ".lsp.json"] {
        if plugin_root.join(unsupported).is_file() {
            bail!(
                "plugin '{}': contains '{}' — only skills and commands are supported",
                name,
                unsupported
            );
        }
    }
    Ok(())
}

/// Symlink entries from `plugin_root/<subdir>/` into `target_dir`.
/// When `dirs_only` is true, non-directory entries are skipped (skills).
/// Returns whether the source directory existed.
fn symlink_entries(
    plugin_root: &std::path::Path,
    subdir: &str,
    target_dir: &std::path::Path,
    dirs_only: bool,
    installed: &mut std::collections::HashSet<std::ffi::OsString>,
) -> Result<bool> {
    let source = plugin_root.join(subdir);
    if !source.is_dir() {
        return Ok(false);
    }

    for entry in fs_err::read_dir(&source)? {
        let entry = entry?;
        if dirs_only && !entry.path().is_dir() {
            continue;
        }

        let name = entry.file_name();
        let symlink_path = target_dir.join(&name);

        // Skip if the symlink already points to the right place
        if let Ok(existing) = fs_err::read_link(&symlink_path) {
            if existing == entry.path() {
                info!("up-to-date {subdir} {}", name.to_string_lossy());
                installed.insert(name);
                continue;
            }
            fs_err::remove_file(&symlink_path)?;
        }

        std::os::unix::fs::symlink(entry.path(), &symlink_path).wrap_err_with(|| {
            format!(
                "failed to create symlink: {} -> {}",
                symlink_path.display(),
                entry.path().display()
            )
        })?;

        info!(
            "installed {subdir} {} -> {}",
            name.to_string_lossy(),
            entry.path().display()
        );
        installed.insert(name);
    }

    Ok(true)
}

/// Remove symlinks in `dir` that point into `cache_dir` but aren't in `keep`.
fn remove_stale_symlinks(
    dir: &std::path::Path,
    cache_dir: &std::path::Path,
    keep: &std::collections::HashSet<std::ffi::OsString>,
    kind: &str,
) -> Result<()> {
    for entry in fs_err::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.symlink_metadata()?.file_type().is_symlink() {
            continue;
        }

        let name = entry.file_name();
        if let Ok(target) = fs_err::read_link(&path)
            && target.starts_with(cache_dir)
            && !keep.contains(&name)
        {
            fs_err::remove_file(&path)?;
            info!("removed stale {kind} symlink: {}", name.to_string_lossy());
        }
    }
    Ok(())
}
