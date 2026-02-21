# pinch

A plugin manager for Claude Code (and [pi](https://github.com/nichochar/pi-coding-agent),
which reads from the same `~/.claude/skills/` directory). Installs skills and
commands as symlinks. Rust, single binary, keep it simple.

Only skills and commands are supported for now. Pinch will fail loudly if asked
to manage anything else.

## Concepts

A **skill** is a directory containing a `SKILL.md` file (and optionally other
files). Claude and pi discover skills by scanning `~/.claude/skills/<name>/`.

A **command** is a slash command — an `.md` file (or directory) that Claude Code
discovers by scanning `~/.claude/commands/`.

A **marketplace** is a git repo that contains one or more plugins, described by
a `.claude-plugin/marketplace.json` manifest. The repo is cloned (regular, not
bare) into pinch's plugins directory; worktrees are created per revision so
multiple versions can coexist. Individual skills are symlinked into
`~/.claude/skills/`.

## Directory layout

```
~/.config/pinch/           # XDG config (conf.d style)
  conf.d/
    base.yml              # any number of .yml files, merged alphabetically
    work.yml

~/.local/share/pinch/      # managed by pinch
  plugins/
    marketplaces/
      github.com-user-skills-repo/     # regular clone (not bare)
        pinch-worktrees/
          abc123def456/                # worktree at rev abc123def456
          def789012345/                # worktree at rev def789012345
    cache/
      github.com-user-skills-repo/     # marketplace name
        jj/
          abc123d/ -> ../../marketplaces/github.com-user-skills-repo/pinch-worktrees/abc123def456/plugins/jj
        gh-pr/
          abc123d/ -> ../../marketplaces/github.com-user-skills-repo/pinch-worktrees/abc123def456/plugins/gh-pr
    installed_plugins.json
    known_marketplaces.json
  pinch-lock.yml          # lockfile

~/.claude/skills/
  jj -> ~/.local/share/pinch/plugins/cache/github.com-user-skills-repo/jj/abc123d/skills/jj
  gh-pr -> ~/.local/share/pinch/plugins/cache/github.com-user-skills-repo/gh-pr/abc123d/skills/gh-pr

~/.claude/commands/
  deploy.md -> ~/.local/share/pinch/plugins/cache/github.com-user-skills-repo/ops/abc123d/commands/deploy.md
```

## Config (manifest)

Config files live in `~/.config/pinch/conf.d/`. All `.yml` files are merged
alphabetically. Duplicate plugin names across files are a hard error.
Marketplace definitions are scoped per-file — the same marketplace name can
appear in different files at different versions.

```yml
marketplaces:
  my-skills:
    src: https://github.com/user/skills-repo
    version: main                 # branch/tag to track
    manifest: custom/path/marketplace.json  # optional, defaults to .claude-plugin/marketplace.json
    plugins:
      - jj
      - gh-pr
```

Plugins are listed under their marketplace. Each plugin inherits the
marketplace's `src`, `version`, and `manifest`. The plugin path within the repo
is discovered from `.claude-plugin/marketplace.json` (or the marketplace's
`manifest` override).

Removing a plugin is done by removing it from the config and re-running
`pinch install`.

## Lockfile

Lives at `~/.local/share/pinch/pinch-lock.yml`. Records the resolved commit
SHA for each skill. `install` reads from the lockfile; `update` writes to it.

```yml
plugins:
  jj:
    marketplace: github.com-user-skills-repo
    src: https://github.com/user/skills-repo
    path: skills/jj
    rev: abc123def456...     # resolved commit SHA

  gh-pr:
    marketplace: github.com-user-skills-repo
    src: https://github.com/user/skills-repo
    path: skills/gh-pr
    rev: abc123def456...
```

## Commands

### `pinch install`

Reads the **lockfile** (not the config). For each entry:

1. Ensures the repo is cloned/fetched and checked out at the locked `rev`.
2. Creates symlinks from `~/.claude/skills/<name>` and `~/.claude/commands/<name>`
   → the corresponding paths in the plugin cache.
3. Removes any symlinks in `~/.claude/skills/` and `~/.claude/commands/` that
   point into `~/.local/share/pinch/plugins/` but are no longer in the lockfile.

Idempotent. Safe to run repeatedly.

Fails loudly if there is no lockfile (tells you to run `pinch update` first).

Optional: `pinch install --update` — runs update then install in one shot.

### `pinch update`

Reads the **config** (merged conf.d). For each skill:

1. Fetches the repo.
2. Resolves the `version` (branch/tag) to a commit SHA.
3. Writes the lockfile.

Does **not** touch symlinks or `~/.claude/skills/`.

Optional: `pinch update --install` — runs update then install in one shot.

Optional: `pinch update <name>` — updates only the named skill(s).

## Development

This project was built entirely through AI-assisted development — pair
programming with Claude Code (and pi). The humans provided direction, design
decisions, and review; the AI wrote the code, tests, and docs.

## Non-goals (for now)

- Dependencies between packages
- Other plugin types (extensions, themes, custom tools)
- A registry / central index
- Running in project-scoped mode (everything is user-global)
