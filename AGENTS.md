# Agents

## Build

```
just
```

This runs `cargo fmt`, `cargo check`, and `cargo clippy`. Always run `just` before committing.

## Project structure

- `src/main.rs` — CLI entry point (clap), `do_update` and `do_install` commands
- `src/config.rs` — loads and merges conf.d/*.yml config files
- `src/lockfile.rs` — reads/writes pinch-lock.yml
- `src/git.rs` — clone, fetch, resolve ref, checkout
- `src/paths.rs` — XDG directory resolution: `plugins_dir()`, `repos_dir()` (marketplaces), `plugin_cache_dir()`

## Design

See README.md for the full design. Key points:

- Only skills and commands are supported. Fail loudly for anything else.
- `update` reads config, resolves git refs to SHAs, writes lockfile. Does not touch symlinks.
- `install` reads lockfile, checks out repos, creates symlinks in ~/.claude/skills/. Does not read config.
- Cleanup removes symlinks pointing into ~/.local/share/pinch/plugins/ that aren't in the lockfile.
- Config is conf.d style — multiple .yml files merged alphabetically, duplicate names are a hard error.

## Plugins directory structure

Marketplaces are stored at `~/.local/share/pinch/plugins/marketplaces/<name>/` as regular git clones (not bare). Worktrees live in `pinch-worktrees/` within each marketplace. A `cache/` symlink tree provides stable paths for Claude Code plugin discovery, pointing into the worktrees. JSON metadata files (`installed_plugins.json`, `known_marketplaces.json`) track what's installed for Claude Code to enumerate.
