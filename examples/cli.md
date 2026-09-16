# wau CLI

A native, CLI-only World of Warcraft addon manager. Configs are TOML — see
`examples/config.toml` and `examples/profiles/example/profile.toml` — and the app never reads
environment variables.

---

## Crate layout (implementation)

- `wau/src/cli/mod.rs` — clap definitions only; no addon logic.
- `wau/src/ctx/mod.rs` — assembles `GlobalConfig` + `ProfileConfig` + DB connection + HTTP
  client + resolver registry into one explicit struct per invocation (`AppCtx`).
- `wau/src/prompts/mod.rs` — interactive confirm/text/password/select-one/select-multiple.
- `wau/src/output/mod.rs` — result reporting, `list` formats.
- `wau/src/app/mod.rs` — command dispatch; `main` stays thin.

All addon/DB/provider logic lives in `libwau`; `wau` only parses args, prompts, and prints.
Commands print plain synchronous output for now; a progress-bar renderer is future polish.

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

There is no `configure` command. A profile is configured by either:

- running `wau init` against it, which prompts interactively and writes the files, or
- hand-writing `<config-dir>/config.toml` (global) and
  `<config-dir>/profiles/<name>/profile.toml` (per-profile) yourself, copying
  `examples/config.toml` / `examples/profiles/example/profile.toml` as a starting point.

`<config-dir>` is the platform-conventional config directory (`~/.config/wau` on Linux,
`~/Library/Application Support/wau` on macOS, `%APPDATA%\wau` on Windows) by default —
override it for a single invocation with `--config` (see above); there is no environment
variable equivalent. The default cache directory is likewise platform-conventional
(`~/.cache/wau` on Linux); override it with `[paths].cache` in `config.toml`. Every command
other than `init` errors out immediately if the active profile's config doesn't exist.

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

Remove installed addons and delete their DB rows. `--keep-folders` leaves the on-disk
directories in place (DB row removed only).

### `wau init`

The only command that bootstraps an unconfigured profile: if `-p`'s profile has no
`profiles/<name>/profile.toml` yet, prompts interactively for the addon directory, game
flavour, and optional GitHub/CurseForge/Wago Addons auth, and writes it (plus `config.toml`,
if that's also missing). Every other command errors out instead — pointing at `wau init` and
`examples/config.toml` / `examples/profiles/example/profile.toml` — if the profile isn't
configured.

Once the profile exists (or was just created), matches un-tracked addon folders (installed by
hand or by another tool) against catalogue/TOC metadata and imports them, in three
decreasing-precision passes (TOC provider-id keys → folder-name subsets → normalized name
match). `-a` / `--auto` picks the top match for every group without prompting;
`--list-unreconciled` only lists what would be matched.

### `wau search <TERM...>`

Fuzzy-search the aggregate catalogue. `-l` / `--limit` (1-20, default 10), `--start-date`,
repeatable `--source`, `--prefer-source`, `--no-exclude-installed`. Presents a
multi-select list; confirmed selections are installed.

### `wau list [ADDON...]` (alias: `wau info` for `-f detailed`)

Lists installed addons. `-f` / `--format {simple,detailed,json}`.

### `wau profile erase`

Deletes the active profile's config and DB after confirmation.

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
