# Plugins Directory Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Make pinch build the `~/.local/share/pinch/plugins/` directory tree that Claude Code expects, replacing Claude Code's native plugin management.

**Architecture:** Regular git clones live at `plugins/marketplaces/<name>/`, worktrees at `pinch-worktrees/<rev>/` inside each clone. Symlinks at `plugins/cache/<mkt>/<plugin>/<rev>/` bridge into worktree plugin paths. Two JSON metadata files (`installed_plugins.json`, `known_marketplaces.json`) tell Claude Code what's installed.

**Tech Stack:** Rust, serde_json for JSON metadata, fs-err for filesystem, xdg for paths.

---

### Task 1: Add marketplace name to config::Plugin

**Files:**
- Modify: `src/config.rs:7-15` (Plugin struct)
- Modify: `src/config.rs:47-62` (load loop)

**Step 1: Add the field**

In `src/config.rs`, add `marketplace` to the `Plugin` struct:

```rust
#[derive(Debug, Clone)]
pub struct Plugin {
    pub name: String,
    pub marketplace: String,
    pub src: String,
    pub version: String,
    pub manifest: Option<String>,
}
```

**Step 2: Thread marketplace name in load()**

Change line 47 from `for (_mkt_name, mkt) in file.marketplaces {` to:

```rust
for (mkt_name, mkt) in file.marketplaces {
```

And update the Plugin construction (line 57-62) to include it:

```rust
plugins.push(Plugin {
    name,
    marketplace: mkt_name.clone(),
    src: mkt.src.clone(),
    version: mkt.version.clone(),
    manifest: mkt.manifest.clone(),
});
```

**Step 3: Build and verify**

Run: `just`
Expected: Clean build, no warnings.

**Step 4: Commit**

Message: `feat: Thread marketplace name through config::Plugin`

---

### Task 2: Add marketplace to lockfile

**Files:**
- Modify: `src/lockfile.rs:12-17` (LockedPlugin struct)
- Modify: `src/main.rs:135-142` (do_update lockfile insert)

**Step 1: Add the field**

In `src/lockfile.rs`, add `marketplace` to `LockedPlugin`:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPlugin {
    pub marketplace: String,
    pub src: String,
    pub path: String,
    pub rev: String,
}
```

**Step 2: Update do_update to populate it**

In `src/main.rs`, update the lockfile insert (lines 135-142):

```rust
lockfile.plugins.insert(
    plugin.name.clone(),
    lockfile::LockedPlugin {
        marketplace: plugin.marketplace.clone(),
        src: plugin.src.clone(),
        path,
        rev,
    },
);
```

**Step 3: Build and verify**

Run: `just`
Expected: Clean build.

**Step 4: Commit**

Message: `feat: Add marketplace name to lockfile`

---

### Task 3: Move repo storage to plugins/marketplaces/

**Files:**
- Modify: `src/paths.rs` (replace repos_dir, add plugins_dir and plugin_cache_dir)
- Modify: `src/git.rs:7-16` (repo_path signature and logic)
- Modify: `src/git.rs:24-45` (clone_or_fetch, drop --bare)
- Modify: `src/git.rs:24-25` (existence check for non-bare repos)
- Modify: `src/git.rs:119-135` (prune, adapt to new repo_path)
- Modify: `src/git.rs:137-195` (visit_repos, detect non-bare repos)
- Modify: `src/main.rs:119-123` (do_update, use marketplace-keyed repo_path)
- Modify: `src/main.rs:150-156,161` (do_install, use marketplace-keyed repo_path)

This is the largest task. It changes where repos live, how they're identified, and switches from bare to regular clones.

**Step 1: Update paths.rs**

Replace the entire file:

```rust
use std::path::PathBuf;

use color_eyre::eyre::{Result, WrapErr};
use xdg::BaseDirectories;

fn base_dirs() -> BaseDirectories {
    BaseDirectories::with_prefix("pinch")
}

pub fn config_dir() -> PathBuf {
    base_dirs()
        .get_config_home()
        .expect("XDG config home not set")
        .join("conf.d")
}

pub fn plugins_dir() -> Result<PathBuf> {
    base_dirs()
        .create_data_directory("plugins")
        .wrap_err("failed to create plugins dir")
}

pub fn repos_dir() -> Result<PathBuf> {
    let dir = plugins_dir()?.join("marketplaces");
    fs_err::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn plugin_cache_dir() -> Result<PathBuf> {
    let dir = plugins_dir()?.join("cache");
    fs_err::create_dir_all(&dir)?;
    Ok(dir)
}

pub fn lockfile_path() -> Result<PathBuf> {
    base_dirs()
        .place_data_file("pinch-lock.yml")
        .wrap_err("failed to create data dir")
}

pub fn skills_dir() -> Result<PathBuf> {
    let path = home_dir().join(".claude").join("skills");
    fs_err::create_dir_all(&path)?;
    Ok(path)
}

pub fn commands_dir() -> Result<PathBuf> {
    let path = home_dir().join(".claude").join("commands");
    fs_err::create_dir_all(&path)?;
    Ok(path)
}

fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .expect("could not determine home directory")
}
```

**Step 2: Update git::repo_path to key on marketplace name**

Replace `repo_path` in `src/git.rs`:

```rust
/// Repo path within the marketplaces directory, keyed by marketplace name.
pub fn repo_path(repos_dir: &Path, marketplace: &str) -> PathBuf {
    repos_dir.join(marketplace)
}
```

**Step 3: Switch clone_or_fetch from bare to regular clone**

In `src/git.rs`, update `clone_or_fetch`. The existence check changes from
`HEAD` file (bare) to `.git` directory (regular clone):

```rust
pub fn clone_or_fetch(repo_path: &Path, src: &str) -> Result<()> {
    if repo_path.join(".git").is_dir() {
        let status = Command::new("git")
            .args(["fetch", "--quiet", "origin"])
            .current_dir(repo_path)
            .status()
            .wrap_err("failed to run git fetch")?;
        if !status.success() {
            bail!("git fetch failed for {}", src);
        }
    } else {
        fs_err::create_dir_all(repo_path)?;
        let status = Command::new("git")
            .args(["clone", "--quiet", src, "."])
            .current_dir(repo_path)
            .status()
            .wrap_err("failed to run git clone")?;
        if !status.success() {
            bail!("git clone failed for {}", src);
        }
    }
    Ok(())
}
```

**Step 4: Update prune to use marketplace name**

In `src/git.rs`, update `prune` to build the used map from marketplace names
instead of source URLs:

```rust
pub fn prune(repos_dir: &Path, lockfile: &crate::lockfile::Lockfile) -> Result<()> {
    use std::collections::{HashMap, HashSet};

    let mut used: HashMap<PathBuf, HashSet<String>> = HashMap::new();
    for plugin in lockfile.plugins.values() {
        let rp = repo_path(repos_dir, &plugin.marketplace);
        used.entry(rp)
            .or_default()
            .insert(plugin.rev[..12].to_string());
    }

    visit_repos(repos_dir, &used)?;

    Ok(())
}
```

**Step 5: Update visit_repos for non-bare repo detection**

In `src/git.rs`, change the `HEAD` file check to `.git` directory check in
`visit_repos`:

```rust
        } else if path.join(".git").is_dir() {
            // This is a regular clone — remove it if no plugins reference it
            if !used.contains_key(&path) {
                fs_err::remove_dir_all(&path)?;
                info!("removed unused repo: {}", path.display());
            }
```

**Step 6: Update do_update callers**

In `src/main.rs`, update `do_update` (around line 123):

```rust
let repo_path = git::repo_path(&repos_dir, &plugin.marketplace);
```

**Step 7: Update do_install callers**

In `src/main.rs`, update `do_install` (around line 161):

```rust
let repo_path = git::repo_path(&repos_dir, &plugin.marketplace);
```

Also remove the `cache_dir` variable (line 156) and update
`remove_stale_symlinks` calls to use `repos_dir` instead:

```rust
remove_stale_symlinks(&skills_dir, &repos_dir, &installed_skills, "skill")?;
remove_stale_symlinks(&commands_dir, &repos_dir, &installed_commands, "command")?;
```

**Step 8: Build and verify**

Run: `just`
Expected: Clean build.

**Step 9: Commit**

Message: `refactor: Move repos to plugins/marketplaces/, switch to regular clones`

---

### Task 4: Build cache symlinks

**Files:**
- Modify: `src/main.rs` (do_install, add cache symlink phase)
- Modify: `src/paths.rs` (already done in task 3)

**Step 1: Add cache symlink creation to do_install**

After the existing skills/commands loop in `do_install`, add a new phase. Insert
after the `remove_stale_symlinks` calls and before `git::prune`:

```rust
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

    // Track for stale cleanup: the marketplace/plugin/version path relative to cache
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
```

**Step 2: Add remove_stale_cache_entries function**

Add at the bottom of `src/main.rs`:

```rust
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
                    info!(
                        "removed stale cache symlink: {}",
                        version_path.display()
                    );
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
```

**Step 3: Build and verify**

Run: `just`
Expected: Clean build.

**Step 4: Commit**

Message: `feat: Build cache/ symlink tree for Claude Code plugin discovery`

---

### Task 5: Write installed_plugins.json and known_marketplaces.json

**Files:**
- Modify: `src/main.rs` (do_install, add JSON writing after cache symlinks)

**Step 1: Add JSON writing to do_install**

After the cache symlink section and before `git::prune`, add:

```rust
// Write metadata JSON files for Claude Code
write_installed_plugins_json(&lockfile, &plugin_cache_dir)?;
write_known_marketplaces_json(&lockfile, &repos_dir)?;
```

**Step 2: Add write_installed_plugins_json function**

```rust
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
    let contents = serde_json::to_string_pretty(&doc).wrap_err("failed to serialize installed_plugins.json")?;
    fs_err::write(&path, contents)?;
    info!("wrote {}", path.display());
    Ok(())
}
```

**Step 3: Add write_known_marketplaces_json function**

```rust
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
```

**Step 4: Add chrono dependency**

In `Cargo.toml`, add:

```toml
chrono = { version = "*", features = ["serde"] }
```

**Step 5: Build and verify**

Run: `just`
Expected: Clean build.

**Step 6: Commit**

Message: `feat: Write installed_plugins.json and known_marketplaces.json`

---

### Task 6: Update README and AGENTS.md

**Files:**
- Modify: `README.md`
- Modify: `AGENTS.md`

**Step 1: Update README.md directory layout**

Replace the directory layout section to reflect the new structure. Update the
`~/.cache/pinch/` section to show it's no longer used, and add the
`~/.local/share/pinch/plugins/` tree.

**Step 2: Update AGENTS.md**

Add a note about the plugins directory structure and the new path functions.

**Step 3: Build and verify**

Run: `just`
Expected: Still clean.

**Step 4: Commit**

Message: `docs: Update README and AGENTS for plugins directory structure`

---

### Task 7: Manual verification

Not a code task. After all commits:

1. Delete `~/.cache/pinch/repos/` and `~/.local/share/pinch/plugins/` to start
   fresh.
2. Run `pinch update --install`.
3. Verify the directory tree at `~/.local/share/pinch/plugins/` matches the
   design.
4. Verify `installed_plugins.json` and `known_marketplaces.json` contain
   correct data.
5. Symlink `~/.claude/plugins` to `~/.local/share/pinch/plugins/` and restart
   Claude Code.
6. Verify plugins load (skills appear in system prompt, commands work).
