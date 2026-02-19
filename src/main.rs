mod config;
mod git;
mod lockfile;
mod marketplace;
mod paths;

use clap::{Parser, Subcommand};
use color_eyre::eyre::{Context, Result, bail};
use tracing::info;

#[derive(Parser)]
#[command(name = "pinch", about = "A plugin manager for Claude Code and pi")]
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
    let config = config::load().context("failed to load config")?;

    let plugins: Vec<_> = if names.is_empty() {
        config.plugins.clone()
    } else {
        config
            .plugins
            .iter()
            .filter(|p| names.contains(&p.name))
            .cloned()
            .collect()
    };

    if plugins.is_empty() && !names.is_empty() {
        bail!("no plugins found matching: {}", names.join(", "));
    }

    let mut lockfile = if names.is_empty() {
        lockfile::Lockfile { plugins: vec![] }
    } else {
        lockfile::load().unwrap_or_else(|_| lockfile::Lockfile { plugins: vec![] })
    };

    let repos_dir = paths::repos_dir();
    std::fs::create_dir_all(&repos_dir)
        .with_context(|| format!("failed to create repos dir: {}", repos_dir.display()))?;

    for plugin in &plugins {
        info!("updating {}...", plugin.name);
        let repo_path = git::repo_path(&repos_dir, &plugin.src);
        git::clone_or_fetch(&repo_path, &plugin.src)?;
        let rev = git::resolve_ref(&repo_path, &plugin.r#ref)?;
        info!("  {} -> {}", plugin.r#ref, &rev[..12]);

        // Checkout to read marketplace.json and discover skill path
        git::checkout(&repo_path, &rev)?;
        let source = marketplace::find_plugin_source(
            &repo_path,
            &plugin.name,
            plugin.marketplace.as_deref(),
        )?;
        let path = source.trim_start_matches("./").to_string();
        info!("  found at {}", path);

        // Remove existing entry for this plugin if doing selective update
        if !names.is_empty() {
            lockfile.plugins.retain(|p| p.name != plugin.name);
        }

        lockfile.plugins.push(lockfile::LockedPlugin {
            name: plugin.name.clone(),
            src: plugin.src.clone(),
            path,
            rev,
        });
    }

    lockfile::save(&lockfile).context("failed to save lockfile")?;
    info!("lockfile updated");
    Ok(())
}

fn do_install() -> Result<()> {
    let lockfile = lockfile::load().context("no lockfile found — run `pinch update` first")?;

    let repos_dir = paths::repos_dir();
    let skills_dir = paths::skills_dir();
    std::fs::create_dir_all(&skills_dir)
        .with_context(|| format!("failed to create skills dir: {}", skills_dir.display()))?;

    let cache_dir = paths::cache_dir();

    for plugin in &lockfile.plugins {
        let repo_path = git::repo_path(&repos_dir, &plugin.src);
        git::checkout(&repo_path, &plugin.rev)?;

        let skill_source = repo_path.join(&plugin.path);
        if !skill_source.is_dir() {
            bail!(
                "plugin '{}': path '{}' does not exist in repo at rev {}",
                plugin.name,
                plugin.path,
                &plugin.rev[..12]
            );
        }

        // Verify it's a skill (has SKILL.md)
        if !skill_source.join("SKILL.md").is_file() {
            bail!(
                "plugin '{}': path '{}' has no SKILL.md — only skills are supported",
                plugin.name,
                plugin.path
            );
        }

        let symlink_path = skills_dir.join(&plugin.name);

        // Remove existing symlink if present
        if symlink_path.symlink_metadata().is_ok() {
            std::fs::remove_file(&symlink_path).with_context(|| {
                format!(
                    "failed to remove existing symlink: {}",
                    symlink_path.display()
                )
            })?;
        }

        std::os::unix::fs::symlink(&skill_source, &symlink_path).with_context(|| {
            format!(
                "failed to create symlink: {} -> {}",
                symlink_path.display(),
                skill_source.display()
            )
        })?;

        info!("installed {} -> {}", plugin.name, skill_source.display());
    }

    // Clean up stale pinch-managed symlinks
    let locked_names: std::collections::HashSet<_> =
        lockfile.plugins.iter().map(|p| p.name.as_str()).collect();

    for entry in std::fs::read_dir(&skills_dir)? {
        let entry = entry?;
        let path = entry.path();

        // Only consider symlinks
        if !path.symlink_metadata()?.file_type().is_symlink() {
            continue;
        }

        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Only touch symlinks pointing into our cache
        if let Ok(target) = std::fs::read_link(&path)
            && target.starts_with(&cache_dir)
            && !locked_names.contains(name_str.as_ref())
        {
            std::fs::remove_file(&path)?;
            info!("removed stale symlink: {}", name_str);
        }
    }

    info!("install complete");
    Ok(())
}
