# wau CLI

A native, CLI-only World of Warcraft addon manager. Configs are TOML — see
`examples/config.toml` and `examples/profiles/example/profile.toml` — and the app never reads
environment variables.

---

## Crate layout (implementation)

- `wau/src/cli/mod.rs` — clap definitions only; no addon logic.
- `wau/src/ctx/mod.rs` — assembles `GlobalConfig` + `ProfileConfig` + lock file + HTTP
  client + resolver registry into one explicit struct per invocation (`AppCtx`).
- `wau/src/prompts/mod.rs` — interactive confirm/select-one (`inquire`-based) plus a plain
  `read_line` for `search`'s freeform paru-style selection prompt. `select-one` is `init`'s
  per-group source picker (unless `--auto`). Profile/global config setup is the only thing
  that's fully non-interactive — it must be hand-written.
- `wau/src/output/mod.rs` — result/`list`/search-result formatting, all `color: bool`-parameterized.
- `wau/src/style.rs` — `owo-colors` wrappers everything in `output/` and most of `app/`'s direct
  `println!`s go through; `style::color_enabled()` (stdout `is_terminal()`, never an environment
  variable) is the one place that decides whether to colorize. `list -f json`/`stats` are never
  colorized — they're machine-readable output.
- `wau/src/app/mod.rs` — command dispatch; `main` stays thin.

All addon/DB/provider logic lives in `libwau`; `wau` only parses args, prompts, and prints.
Colorized output styled after `paru`'s (see `wau search`, below) — a progress-bar renderer for
long-running operations is still future polish.

---

## Global flags

- `--version`
- `--config <PATH>` — read `config.toml` from `<PATH>` instead of the platform-conventional
  config dir; `profiles/` is then resolved as `<PATH>`'s sibling directory. Mainly for
  integration tests that need an isolated config location.
- `-p` / `--profile <NAME-OR-PATH>` — target profile (default `default`); see
  `examples/profiles/example/profile.toml`. A bare name is looked up as
  `<config-dir>/profiles/<name>/profile.toml`; a value containing a path separator or ending in
  `.toml` is read directly from that path instead (its `lock.toml` sibling is the installed-package
  lock file — see `examples/profiles/example/lock.toml`), bypassing name-based lookup entirely —
  again mainly for integration tests pointing at a fixture file.

Log verbosity is not a CLI flag — it comes from `[logging].level` in `config.toml` (default
`warn` if unset). There is no `$RUST_LOG` support either: `wau` never reads environment
variables, full stop.

## Configuration

There is no `configure` command and no interactive bootstrap of any kind. A profile is
configured by hand-writing `<config-dir>/config.toml` (global) and
`<config-dir>/profiles/<name>/profile.toml` (per-profile) yourself, copying
`examples/config.toml` / `examples/profiles/example/profile.toml` as a starting point.

`<config-dir>` is the platform-conventional config directory (`~/.config/wau` on Linux,
`~/Library/Application Support/wau` on macOS, `%APPDATA%\wau` on Windows) by default —
override it for a single invocation with `--config` (see above); there is no environment
variable equivalent. The default cache directory is likewise platform-conventional
(`~/.cache/wau` on Linux); override it with `[paths].cache` in `config.toml`. Every command
except `init` errors out immediately if the active (`-p`-selected) profile's config doesn't
exist; `init` ignores `-p` and instead reports if there are no profiles configured at all (see
below).

## Commands

### `wau install <ADDON...>`

Install one or more `source:alias` definitions (or bare aliases/URLs a source's
`get_alias_from_url` can parse). `--replace` overwrites unreconciled on-disk folders that
collide with the new install; `--dry-run` resolves and reports without installing.

### `wau sync [ADDON...]`

Update installed addons to the latest version per their stored strategies. With no
arguments, updates everything installed (skipping up-to-date/pinned packages). `--dry-run`
reports without installing.

### `wau remove <ADDON...>`

Remove installed addons and delete their lock file entries. `--keep-folders` leaves the on-disk
directories in place (lock file entry removed only).

### `wau replace <OLD> <NEW>`

Switches one installed addon to a different source (e.g. `wau replace curse:big-wigs
github:BigWigsMods/BigWigs`) as one operation: resolves `NEW`, downloads and installs it, then
removes `OLD`'s lock file entry — rather than the folder-conflict-prone sequencing of doing it
by hand as two separate `remove`/`install` calls. `OLD` is looked up like `remove`'s targets
(works even if its source was since removed/disabled); `NEW` must resolve against a live
source, same as `install`. Neither side's pin (`version_eq`) carries over automatically — give
`NEW` its own `#version_eq=...` fragment if you want it pinned. `OLD`'s version-history log
entries aren't deleted, but they also don't transfer to `NEW`'s identity — they're just left
behind, keyed to the old `(source, id)`.

### `wau init`

Reconciles **every** configured profile (`<config-dir>/profiles/*/profile.toml`) in one run —
ignores `-p`/`--profile`, since there's no single "active" profile for this command; prints
`== <name> ==` between profiles when there's more than one. If no profiles exist yet, says so
and exits — profiles are never created by `init` or anything else; hand-write them (see
`examples/config.toml` / `examples/profiles/example/profile.toml`).

For each profile, matches un-tracked addon folders (installed by hand or by another tool)
against catalogue/TOC metadata and imports them, in three decreasing-precision passes (TOC
provider-id keys → folder-name subsets → normalized name match). For each matched group,
prompts a single-select of the candidate sources (priority order) and then confirms before
installing the batch; `-a` / `--auto` picks the top-priority candidate for every group instead,
with no prompting at all. `--list-unreconciled` only lists what would be matched, for every
profile, without installing anything, prompting, or touching the network.

### `wau search <TERM...>`

Fuzzy-search the aggregate catalogue. `-l` / `--limit` (1-20, default 10), `--start-date`,
repeatable `--source`, `--prefer-source`, `--no-exclude-installed`. Results print `paru`-style:
numbered top to bottom from least to most relevant, so the best match is `1`, right above the
input prompt — each entry shows `source/slug [downloads↓]` (`[Installed]` appended if
applicable) with its display name indented below (the catalogue has no version/description to
show, unlike a real package repository, so those don't appear here the way they do in `paru`).
Pick addons to install the same way `paru` does: `:: Packages to install (eg: 1 2 3, 1-3):`
takes space/comma-separated numbers and/or `a-b` ranges; typing nothing installs nothing, no
separate confirmation step.

### `wau list [ADDON...]` (alias: `wau info` for `-f detailed`)

Lists installed addons. `-f` / `--format {simple,detailed,json}`.

### `wau profile erase`

Deletes the active profile's config and lock file. No confirmation prompt.

### `wau stats`

Prints the active profile config, all known profile names, and source metadata as JSON.

---

## Out of scope

- GUI, background daemon, tray integration.
- A WeakAuras Companion source/updater.
- A runtime plugin loader — third-party addon sources are added as Cargo features/crates
  instead.
- SavedVariables backup/restore — removed addon folders are trashed to a temp dir rather than
  hard-deleted, which is the only safety net for destructive operations.
