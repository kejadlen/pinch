#![deny(clippy::disallowed_methods)]

mod config;
mod git;
mod lockfile;
mod marketplace;
mod paths;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Result, bail};
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
    let all_plugins = config::load().context("failed to load config")?;

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

    lockfile::save(&lockfile).context("failed to save lockfile")?;
    info!("lockfile updated");
    Ok(())
}

fn do_install() -> Result<()> {
    let lockfile = lockfile::load().context("no lockfile found — run `pinch update` first")?;

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

        // Reject unsupported plugin types
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

        // Symlink each skill under skills/
        let skills_source = plugin_root.join("skills");
        if skills_source.is_dir() {
            for entry in fs_err::read_dir(&skills_source)? {
                let entry = entry?;
                if !entry.path().is_dir() {
                    continue;
                }

                let skill_name = entry.file_name();
                let symlink_path = skills_dir.join(&skill_name);

                // Remove existing symlink if present
                if symlink_path.symlink_metadata().is_ok() {
                    fs_err::remove_file(&symlink_path)?;
                }

                std::os::unix::fs::symlink(entry.path(), &symlink_path).with_context(|| {
                    format!(
                        "failed to create symlink: {} -> {}",
                        symlink_path.display(),
                        entry.path().display()
                    )
                })?;

                info!(
                    "installed skill {} -> {}",
                    skill_name.to_string_lossy(),
                    entry.path().display()
                );
                installed_skills.insert(skill_name);
            }
        }

        // Symlink each command under commands/
        let commands_source = plugin_root.join("commands");
        if commands_source.is_dir() {
            for entry in fs_err::read_dir(&commands_source)? {
                let entry = entry?;
                let command_name = entry.file_name();
                let symlink_path = commands_dir.join(&command_name);

                // Remove existing symlink if present
                if symlink_path.symlink_metadata().is_ok() {
                    fs_err::remove_file(&symlink_path)?;
                }

                std::os::unix::fs::symlink(entry.path(), &symlink_path).with_context(|| {
                    format!(
                        "failed to create symlink: {} -> {}",
                        symlink_path.display(),
                        entry.path().display()
                    )
                })?;

                info!(
                    "installed command {} -> {}",
                    command_name.to_string_lossy(),
                    entry.path().display()
                );
                installed_commands.insert(command_name);
            }
        }

        if !skills_source.is_dir() && !commands_source.is_dir() {
            bail!(
                "plugin '{}': no skills/ or commands/ directory in '{}'",
                name,
                plugin.path
            );
        }
    }

    // Clean up stale skill symlinks
    for entry in fs_err::read_dir(&skills_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.symlink_metadata()?.file_type().is_symlink() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if let Ok(target) = fs_err::read_link(&path)
            && target.starts_with(&cache_dir)
            && !installed_skills.contains(&name)
        {
            fs_err::remove_file(&path)?;
            info!("removed stale skill symlink: {}", name_str);
        }
    }

    // Clean up stale command symlinks
    for entry in fs_err::read_dir(&commands_dir)? {
        let entry = entry?;
        let path = entry.path();

        if !path.symlink_metadata()?.file_type().is_symlink() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        if let Ok(target) = fs_err::read_link(&path)
            && target.starts_with(&cache_dir)
            && !installed_commands.contains(&name)
        {
            fs_err::remove_file(&path)?;
            info!("removed stale command symlink: {}", name_str);
        }
    }

    // Clean up stale worktrees and unused repos
    git::prune(&repos_dir, &lockfile)?;

    info!("install complete");
    Ok(())
}
