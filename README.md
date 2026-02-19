# pinch

A plugin manager for Claude Code (and [pi](https://github.com/nichochar/pi-coding-agent),
which reads from the same `~/.claude/skills/` directory). Installs skills as
symlinks. Rust, single binary, keep it simple.

Only skills are supported for now. Pinch will fail loudly if asked to manage
anything else.

## Concepts

A **skill** is a directory containing a `SKILL.md` file (and optionally other
files). Claude and pi discover skills by scanning `~/.claude/skills/<name>/`.

A **marketplace** is a git repo that contains one or more plugins, described by
a `.claude-plugin/marketplace.json` manifest.

A **package** is a git repo that contains one or more skills. The repo is cloned
(bare) into pinch's cache; worktrees are created per revision so multiple
versions can coexist. Individual skills are symlinked into `~/.claude/skills/`.

## Directory layout

```
~/.config/pinch/           # XDG config (conf.d style)
  conf.d/
    base.yml              # any number of .yml files, merged alphabetically
    work.yml

~/.cache/pinch/            # managed by pinch
  repos/
    github.com/
      user/repo.git/                   # bare clone
        pinch-worktrees/
          abc123def456/                # worktree at rev abc123def456
          def789012345/                # worktree at rev def789012345

~/.local/share/pinch/
  pinch-lock.yml          # lockfile

~/.claude/skills/
  jj -> ~/.cache/pinch/repos/github.com/user/repo.git/pinch-worktrees/abc123def456/plugins/jj/skills/jj
  gh-pr -> ~/.cache/pinch/repos/github.com/user/repo.git/pinch-worktrees/def789012345/plugins/gh-pr/skills/gh-pr
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
    version: main                 # default version (branch/tag) for plugins from this marketplace
    manifest: custom/path/marketplace.json  # optional, defaults to .claude-plugin/marketplace.json

plugins:
  jj:
    marketplace: my-skills

  gh-pr:
    marketplace: my-skills
```

Every plugin references a named marketplace (inheriting `src`, `version`, and
`manifest`). The plugin path within the repo is discovered from
`.claude-plugin/marketplace.json` (or the marketplace's `manifest` override).

Removing a skill is done by removing it from the config and re-running
`pinch install`.

## Lockfile

Lives at `~/.local/share/pinch/pinch-lock.yml`. Records the resolved commit
SHA for each skill. `install` reads from the lockfile; `update` writes to it.

```yml
plugins:
  jj:
    src: https://github.com/user/skills-repo
    path: skills/jj
    rev: abc123def456...     # resolved commit SHA

  gh-pr:
    src: https://github.com/user/skills-repo
    path: skills/gh-pr
    rev: abc123def456...
```

## Commands

### `pinch install`

Reads the **lockfile** (not the config). For each entry:

1. Ensures the repo is cloned/fetched and checked out at the locked `rev`.
2. Creates a symlink from `~/.claude/skills/<name>` → the skill's path in the
   cached repo.
3. Removes any symlinks in `~/.claude/skills/` that point into
   `~/.cache/pinch/` but are no longer in the lockfile.

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
- Non-skill plugin types (extensions, themes, custom tools)
- A registry / central index
- Running in project-scoped mode (everything is user-global)
