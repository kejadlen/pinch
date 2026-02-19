# pinch

A plugin manager for Claude Code (and [pi](https://github.com/nichochar/pi-coding-agent),
which reads from the same `~/.claude/skills/` directory). Installs skills as
symlinks. Rust, single binary, keep it simple.

Only skills are supported for now. Pinch will fail loudly if asked to manage
anything else.

## Concepts

A **skill** is a directory containing a `SKILL.md` file (and optionally other
files). Claude and pi discover skills by scanning `~/.claude/skills/<name>/`.

A **package** is a git repo that contains one or more skills. The repo is cloned
into pinch's cache; individual skills are symlinked into `~/.claude/skills/`.

## Directory layout

```
~/.config/pinch/           # XDG config (conf.d style)
  conf.d/
    base.yml              # any number of .yml files, merged alphabetically
    work.yml

~/.cache/pinch/            # managed by pinch
  repos/
    github.com/
      user/repo/           # bare or full clone

~/.local/share/pinch/
  pinch-lock.yml          # lockfile

~/.claude/skills/
  jj -> ~/.cache/pinch/repos/github.com/user/repo/skills/jj
  gh-pr -> ~/.cache/pinch/repos/github.com/user/repo/skills/gh-pr
```

## Config (manifest)

Config files live in `~/.config/pinch/conf.d/`. All `.yml` files are merged
alphabetically. Duplicate skill names across files are a hard error.

```yml
plugins:
  jj:
    src: https://github.com/user/skills-repo
    ref: main                 # branch or tag; resolved to a commit SHA in lockfile

  gh-pr:
    src: https://github.com/user/skills-repo
    ref: v1.2.0
    marketplace: custom/path/marketplace.json  # optional override
```

The plugin path within the repo is discovered from the marketplace manifest
(`.claude-plugin/marketplace.json` by default). The `marketplace` field lets you
override this path per plugin.

Removing a skill is done by removing it from the config and re-running
`pinch install`.

## Lockfile

Lives at `~/.local/share/pinch/pinch-lock.yml`. Records the resolved commit
SHA for each skill. `install` reads from the lockfile; `update` writes to it.

```yml
plugins:
  - name: jj
    src: https://github.com/user/skills-repo
    path: skills/jj
    rev: abc123def456...     # resolved commit SHA

  - name: gh-pr
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
2. Resolves the `ref` (branch/tag) to a commit SHA.
3. Writes the lockfile.

Does **not** touch symlinks or `~/.claude/skills/`.

Optional: `pinch update --install` — runs update then install in one shot.

Optional: `pinch update <name>` — updates only the named skill(s).

## Non-goals (for now)

- Dependencies between packages
- Non-skill plugin types (extensions, themes, custom tools)
- A registry / central index
- Running in project-scoped mode (everything is user-global)
