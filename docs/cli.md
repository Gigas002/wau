# wau CLI

A native, CLI-only World of Warcraft addon manager. Configs are TOML — see `examples/config.toml` and `examples/profiles/example/profile.toml`.

---

## Global flags

- `--version`
- `--config <PATH>` — read `config.toml` from `<PATH>` instead of the platform-conventional
  config dir; `profiles/` is then resolved as `<PATH>`'s sibling directory. Mainly for
  integration tests that need an isolated config location.
- `-p` / `--profile <NAME-OR-PATH>` — target profile; see `examples/profiles/example/profile.toml`.
  A bare name is looked up as `<config-dir>/profiles/<name>/profile.toml`; a value containing a
  path separator or ending in `.toml` is read directly from that path instead (its `lock.toml`
  sibling is the installed-package lock file — see `examples/profiles/example/lock.toml`),
  bypassing name-based lookup entirely — again mainly for integration tests pointing at a fixture
  file. `init` is the one exception — omitting `-p` there reconciles _every_
  configured profile in turn rather than erroring on "several configured"; give it explicitly to
  restrict `init` to just that one profile (see below).

## Configuration

`<config-dir>` is the platform-conventional config directory (`~/.config/wau` on Linux, `~/Library/Application Support/wau` on macOS, `%APPDATA%\wau` on Windows) by default — override it for a single invocation with `--config` (see above)

The default cache directory is likewise platform-conventional (`~/.cache/wau` on Linux)

Every command errors out immediately if the profile it needs doesn't exist; without `-p`, `init` reports instead if there are no profiles configured at all, rather than erroring.

## Commands

### `wau install <ADDON...>`

Install one or more `source:alias` definitions (or bare aliases/URLs a source's `get_alias_from_url` can parse).

`--replace` overwrites unreconciled on-disk folders that collide with the new install.

`--dry-run` resolves and reports without installing.

### `wau sync [ADDON...]`

Update installed addons to the latest version per their stored strategies.

With no arguments, updates everything installed (skipping up-to-date/pinned packages).

`--dry-run` reports without installing.

### `wau remove <ADDON...>`

Remove installed addons and delete their lock file entries.

`--keep-folders` leaves the on-disk directories in place (lock file entry removed only).

### `wau replace <OLD> <NEW>`

Switches one installed addon to a different source, e.g:

```sh
wau replace curse:big-wigs github:BigWigsMods/BigWigs
```

### `wau init`

With `-p`/`--profile`, reconciles just that one profile, same as every other command.

Without it, reconciles **every** configured profile (`<config-dir>/profiles/*/profile.toml`) in one run, printing `== <name> ==` between profiles when there's more than one.

For each profile, matches un-tracked addon folders (installed by hand or by another tool) against catalogue/TOC metadata and imports them, in three decreasing-precision passes (TOC provider-id keys → folder-name subsets → normalized name match).

For each matched group, prompts a single-select of the candidate sources (priority order) and then confirms before installing the batch; `-a` / `--auto` picks the top-priority candidate for every group instead, with no prompting at all.

`--list-unreconciled` only lists what would be matched, for every profile, without installing anything, prompting, or touching the network.

### `wau search <TERM...>`

Fuzzy-search the aggregate catalogue. `-l` / `--limit` (1-20, default 10), `--start-date`, repeatable `--source`, `--prefer-source`, `--exclude-installed`.

Results print `paru`-style: numbered top to bottom from least to most relevant, so the best match is `1`, right above the input prompt — each entry shows `source/slug [downloads↓]` (`[Installed: <version>]` appended when it matches an installed package — the one place a real version shows up, since it comes from the lock file, not the catalogue) with its display name indented below (the catalogue itself has no version/description to show, unlike a real package repository, so those don't appear here the way they do in `paru`).

Pick addons to install the same way `paru` does: `:: Packages to install (eg: 1 2 3, 1-3):` takes space/comma-separated numbers and/or `a-b` ranges; typing nothing installs nothing, no separate confirmation step.

### `wau list [ADDON...]` (alias: `wau info` for `-f detailed`)

Lists installed addons. `-f` / `--format {simple,detailed,json}`. `simple` (the default) is `source:slug <version>` per line — the URI first so it still parses as `source:slug` on its own (e.g. piped elsewhere), version trailing it.
