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
        let repo_path = git::repo_path(&repos_dir, &plugin.marketplace);
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
                marketplace: plugin.marketplace.clone(),
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
    let mut installed_skills = std::collections::HashSet::new();
    let mut installed_commands = std::collections::HashSet::new();

    for (name, plugin) in &lockfile.plugins {
        let repo_path = git::repo_path(&repos_dir, &plugin.marketplace);
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

    remove_stale_symlinks(&skills_dir, &repos_dir, &installed_skills, "skill")?;
    remove_stale_symlinks(&commands_dir, &repos_dir, &installed_commands, "command")?;

    // Build plugins/cache/ symlinks for Claude Code
    let plugin_cache_dir = paths::plugin_cache_dir()?;
    let mut installed_cache_entries = std::collections::HashSet::new();

    for (name, plugin) in &lockfile.plugins {
        let repo_path = git::repo_path(&repos_dir, &plugin.marketplace);
        let wt_path = git::worktree_path(&repo_path, &plugin.rev);
        let plugin_root = wt_path.join(&plugin.path);

        // cache/<marketplace>/<plugin>/<rev_short>/
        let cache_mkt_dir = plugin_cache_dir.join(&plugin.marketplace);
        let cache_plugin_dir = cache_mkt_dir.join(name);
        let cache_version_dir = cache_plugin_dir.join(&plugin.rev[..12]);

        fs_err::create_dir_all(&cache_plugin_dir)?;

        // Track for stale cleanup
        installed_cache_entries.insert(cache_version_dir.clone());

        // Skip if already correct
        if let Ok(existing) = fs_err::read_link(&cache_version_dir) {
            if existing == plugin_root {
                info!("up-to-date cache entry {}/{}", plugin.marketplace, name);
                continue;
            }
            fs_err::remove_file(&cache_version_dir)?;
        }

        std::os::unix::fs::symlink(&plugin_root, &cache_version_dir).wrap_err_with(|| {
            format!(
                "failed to create cache symlink: {} -> {}",
                cache_version_dir.display(),
                plugin_root.display()
            )
        })?;

        info!(
            "cache {}/{} -> {}",
            plugin.marketplace,
            name,
            plugin_root.display()
        );
    }

    // Remove stale cache symlinks
    remove_stale_cache_entries(&plugin_cache_dir, &installed_cache_entries)?;

    // Write metadata JSON files for Claude Code
    write_installed_plugins_json(&lockfile, &plugin_cache_dir)?;
    write_known_marketplaces_json(&lockfile, &repos_dir)?;

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
// TODO: maybe re-use frork-lib for symlink management in the future
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

/// Remove cache/<mkt>/<plugin>/<version> symlinks not in `keep`,
/// then clean up empty parent directories.
fn remove_stale_cache_entries(
    cache_dir: &std::path::Path,
    keep: &std::collections::HashSet<std::path::PathBuf>,
) -> Result<()> {
    if !cache_dir.is_dir() {
        return Ok(());
    }

    // Walk cache/<mkt>/<plugin>/ looking for version symlinks
    for mkt_entry in fs_err::read_dir(cache_dir)? {
        let mkt_entry = mkt_entry?;
        let mkt_path = mkt_entry.path();
        if !mkt_path.is_dir() {
            continue;
        }

        for plugin_entry in fs_err::read_dir(&mkt_path)? {
            let plugin_entry = plugin_entry?;
            let plugin_path = plugin_entry.path();
            if !plugin_path.is_dir() && !plugin_path.symlink_metadata()?.file_type().is_symlink() {
                continue;
            }

            for version_entry in fs_err::read_dir(&plugin_path)? {
                let version_entry = version_entry?;
                let version_path = version_entry.path();

                if version_path.symlink_metadata()?.file_type().is_symlink()
                    && !keep.contains(&version_path)
                {
                    fs_err::remove_file(&version_path)?;
                    info!("removed stale cache symlink: {}", version_path.display());
                }
            }

            // Remove plugin dir if empty
            if fs_err::read_dir(&plugin_path)?.next().is_none() {
                fs_err::remove_dir(&plugin_path)?;
            }
        }

        // Remove marketplace dir if empty
        if fs_err::read_dir(&mkt_path)?.next().is_none() {
            fs_err::remove_dir(&mkt_path)?;
        }
    }

    Ok(())
}

fn write_installed_plugins_json(
    lockfile: &lockfile::Lockfile,
    plugin_cache_dir: &std::path::Path,
) -> Result<()> {
    use std::collections::BTreeMap;

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let mut plugins: BTreeMap<String, Vec<serde_json::Value>> = BTreeMap::new();
    for (name, plugin) in &lockfile.plugins {
        let key = format!("{}@{}", name, plugin.marketplace);
        let install_path = plugin_cache_dir
            .join(&plugin.marketplace)
            .join(name)
            .join(&plugin.rev[..12]);

        plugins.insert(
            key,
            vec![serde_json::json!({
                "scope": "user",
                "installPath": install_path.to_string_lossy(),
                "version": &plugin.rev[..12],
                "installedAt": &now,
                "lastUpdated": &now,
                "gitCommitSha": &plugin.rev,
            })],
        );
    }

    let doc = serde_json::json!({
        "version": 2,
        "plugins": plugins,
    });

    let plugins_dir = paths::plugins_dir()?;
    let path = plugins_dir.join("installed_plugins.json");
    let contents = serde_json::to_string_pretty(&doc)
        .wrap_err("failed to serialize installed_plugins.json")?;
    fs_err::write(&path, contents)?;
    info!("wrote {}", path.display());
    Ok(())
}

fn write_known_marketplaces_json(
    lockfile: &lockfile::Lockfile,
    repos_dir: &std::path::Path,
) -> Result<()> {
    use std::collections::BTreeMap;

    let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);

    let mut marketplaces: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for plugin in lockfile.plugins.values() {
        if marketplaces.contains_key(&plugin.marketplace) {
            continue;
        }

        let install_location = repos_dir.join(&plugin.marketplace);

        // Derive github source from URL if possible
        let source = if let Some(path) = plugin.src.strip_prefix("https://github.com/") {
            let repo = path.trim_end_matches(".git");
            serde_json::json!({
                "source": "github",
                "repo": repo,
            })
        } else {
            serde_json::json!({
                "source": "git",
                "url": &plugin.src,
            })
        };

        marketplaces.insert(
            plugin.marketplace.clone(),
            serde_json::json!({
                "source": source,
                "installLocation": install_location.to_string_lossy(),
                "lastUpdated": &now,
            }),
        );
    }

    let plugins_dir = paths::plugins_dir()?;
    let path = plugins_dir.join("known_marketplaces.json");
    let contents = serde_json::to_string_pretty(&marketplaces)
        .wrap_err("failed to serialize known_marketplaces.json")?;
    fs_err::write(&path, contents)?;
    info!("wrote {}", path.display());
    Ok(())
}
