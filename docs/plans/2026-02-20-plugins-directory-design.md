# Manage ~/.claude/plugins via pinch

## Problem

Claude Code discovers plugins from `~/.claude/plugins/`, a directory it manages
with its own git cloning, caching, and JSON metadata. Pinch already manages
skills and commands from the same plugin repos. Running both systems in parallel
duplicates git operations and splits control of the same data.

This design makes pinch the sole manager of the plugins directory structure,
so Claude Code reads from a tree pinch builds.

## Directory layout

```
~/.local/share/pinch/plugins/
├── marketplaces/
│   └── <marketplace-name>/                 # regular git clone (not bare)
│       ├── .git/
│       ├── .claude-plugin/
│       │   └── marketplace.json            # visible to Claude Code
│       ├── pinch-worktrees/
│       │   └── <rev_short>/                # worktree checkout (all plugins at this rev)
│       │       ├── plugin-a/
│       │       │   ├── .claude-plugin/plugin.json
│       │       │   └── skills/
│       │       └── plugin-b/
│       │           └── ...
│       └── ...
├── cache/
│   └── <marketplace-name>/
│       └── <plugin-name>/
│           └── <rev_short>/                # symlink into worktree plugin path
├── installed_plugins.json
└── known_marketplaces.json
```

One regular (non-bare) clone per marketplace source URL, stored at
`marketplaces/<name>/`. Worktrees live inside at `pinch-worktrees/<rev_short>/`.
The `cache/` tree contains symlinks that bridge into the worktrees, shaped to
match what Claude Code expects: `cache/<marketplace>/<plugin>/<version>/`.

Two JSON files at the root tell Claude Code what's installed:

- `installed_plugins.json` (version 2) — one entry per plugin with
  `installPath`, `version`, `gitCommitSha`, timestamps, and scope
- `known_marketplaces.json` — one entry per marketplace with `source` and
  `installLocation`

`install-counts-cache.json` is omitted; Claude Code fetches it independently.

### TODO

Verify whether Claude Code reads `marketplaces/` at runtime. If not, the
regular clone can revert to bare without losing functionality.

## Code changes

### paths.rs

- `plugins_dir()` — returns `~/.local/share/pinch/plugins/` (via XDG data home)
- `repos_dir()` — returns `plugins_dir()/marketplaces/` (replaces
  `~/.cache/pinch/repos/`)
- `plugin_cache_dir()` — returns `plugins_dir()/cache/`
- `repo_path()` keyed on marketplace name, not URL
- `cache_dir()` updated for stale symlink detection (now points into
  `plugins_dir()/marketplaces/`)
- `skills_dir()`, `commands_dir()` unchanged

`~/.cache/pinch/repos/` is deprecated and no longer used.

### config.rs

Thread marketplace name through to the `Plugin` struct. The `_mkt_name`
variable in the config loading loop becomes a real field.

### lockfile.rs

Add `marketplace: String` to `LockedPlugin`. Populated during `do_update` from
the config's marketplace name.

### git.rs

- Drop `--bare` flag from `clone_or_fetch`
- `repo_path()` takes marketplace name instead of deriving path from URL
- Worktree and prune logic structurally unchanged, just new base paths

### main.rs

`do_update` writes the marketplace name into each lockfile entry.

`do_install` gains a new phase after skills/commands symlinking:

1. For each locked plugin, create a symlink at
   `cache/<marketplace>/<plugin>/<rev_short>/` pointing into the worktree at
   the plugin's path.
2. Write `installed_plugins.json` with version 2 format — one entry per locked
   plugin, `installPath` pointing to the cache symlink path.
3. Write `known_marketplaces.json` — one entry per marketplace, source derived
   from config.
4. Clean up stale symlinks in the `cache/` tree (same pattern as skills/commands
   cleanup).

## What this does NOT do

- Symlink `~/.claude/plugins` to the pinch-managed directory. That's a manual
  step once the structure is verified to work.
- Remove the existing skills/commands symlinking. Both approaches coexist until
  the plugins structure proves sufficient.
- Handle `install-counts-cache.json`. Claude Code manages that file itself.
