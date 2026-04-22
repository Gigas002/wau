# wau — Rust architecture + implementation plan

This document is both a **human roadmap** and an **agent playbook**: each step is sized for a focused implementation session, ends in a **verified** state (**build** + **fmt/clippy** + **tests** where applicable), and defines **how to verify** it.

It is intentionally modeled after the execution discipline in:

- `imgvwr` — phased, verifiable steps with strict crate boundaries
- `geotiles-rs` — library-first architecture, feature gates, test discipline
- `tofi-rs` — migration/release discipline and “clean main branch” hygiene (including post-migration cleanup)

Reference plans:

- `imgvwr` plan: `docs/IMV_RS_PLAN.md`
- `geotiles-rs` plan: `docs/GEOTILES_RUST_MIGRATION_PLAN.md`
- `tofi-rs` plans: `docs/RUST_MIGRATION_PLAN.md`, `docs/POST_MIGRATION_PLAN.md`

Implementation references (for behavior and provider semantics, not for copying architecture 1:1):

- [ajour](https://github.com/ajour/ajour) (deprecated, still useful for ideas)
- [instawow](https://github.com/layday/instawow)
- [wowup](https://github.com/WowUp)

CLI UX inspiration and “power-user” ergonomics:

- [paru](https://github.com/morganamilo/paru)

---

## 1. Goals and constraints

### 1.1 Goals

- **Two-crate workspace**:
  - **`libwau`**: addon domain model, manifest/lock/config resolution, provider abstraction, filesystem operations (install/update/remove), and tests.
  - **`wau`**: thin CLI wrapper (arg parsing, printing, interactive prompts).
- **Multi-provider**: support multiple addon sources via a stable provider trait (CurseForge, GitHub releases, WowInterface, Wago, etc.). Providers are _pluggable_, not hardcoded.
- **Multi-flavor**: support multiple WoW installs and “flavors” (retail/classic-era/classic-cata/etc.) in a uniform way.
- **Multi-channel**: support update channels (e.g. `stable`, `beta`, `alpha`, `git`) and per-addon overrides.
- **Own manifest + package management**:
  - **Manifest** defines _desired state_ (what addons, which provider, channel, flavor constraints, optional pinning).
  - **Lock** records _resolved state_ (exact release/build, download URL, checksums, installed folders, timestamps).
- **Settings backup/restore**:
  - Support backup/restore of addon settings (WoW `WTF/Account/.../SavedVariables/*.lua` plus per-character SavedVariables where applicable).
  - Store backups in a wau-managed location (in cache or user-specified backup dir) with retention policy (e.g. keep last \(N\) per addon).
  - Backups are taken automatically on update/remove (configurable), and can be invoked manually.
- **Previous-version installation**:
  - Support installing an older version (“downgrade”) and reinstalling any prior resolved version from lock history.
  - Support “pinning” an addon to a specific version/release id in the manifest.
- **Provider switching (source migration)**:
  - Allow a logical addon to switch providers (e.g. CurseForge → Wago) while keeping its installed folder identity and settings intact.
  - Switching is explicit and recorded (manifest change + lock records previous provider resolution).
- **Paru-like CLI design**: terse defaults for power users, clear subcommands and short flags, consistent exit codes, useful `--help`, optional color, non-interactive mode.
- **Safe installs**:
  - Never corrupt `Interface/AddOns` on partial failure.
  - Staging installs in a temp dir; only “swap in” on success.
  - Track installed directories per addon.
- **Platform support**:
  - **v0 target**: Linux.
  - Post-v0: Windows and macOS.

### 1.1.1 Discipline (non-negotiable)

- **Library-first**: all business logic and I/O is in `libwau`; `wau` is a thin presentation/wiring layer.
- **Step sizing**: follow "small, verifiable steps" (imgvwr/geotiles) rather than big-bang refactors.
- **Release hygiene**: keep `main` clean of "planning diary noise" and migration-era scaffolding (tofi-rs). Plans live in `docs/`, and historical plans can be moved to a branch/tag when stable.
- **Stay slim**: keep each module, function, and file focused and minimal. Prefer many small, clear units over few large ones.
- **Naming**: short but descriptive names; optimize for the reader, not the writer.

### 1.2 Non-goals

- GUI, background daemon, tray integration.
- Full “catalog browser” UI; CLI only.
- Syncing addons across machines automatically (can be layered later via manifest + lock).
- Supporting every provider immediately: the architecture must make it easy, but v0 ships with a minimal set of working providers.

### 1.3 Definitions

- **Addon**: an installed package in `Interface/AddOns/` (usually multiple folders) described by one or more `.toc` files.
- **Provider**: remote index + artifact distribution service (CurseForge, GitHub, …).
- **Flavor**: a WoW “product” install folder (e.g. `_retail_`, `_classic_`).
- **Channel**: release stream preference (stable/beta/alpha/git).

### 1.4 Compatibility targets (behavioral reference only)

The following projects are referenced for **semantics and UX ideas**, not for architecture replication:

- [ajour](https://github.com/ajour/ajour): how providers model release selection, and practical edge cases in addon installs.
- [instawow](https://github.com/layday/instawow): manifest/lock “desired vs resolved state” approach and reproducibility.
- [wowup](https://github.com/WowUp): provider coverage patterns, metadata choices, and install folder handling.
- [paru](https://github.com/morganamilo/paru): CLI ergonomics for a “package-manager-like” workflow (sync, search, list, info, noninteractive).

---

## 2. Repository layout (target)

```text
wau/
  Cargo.toml                  # workspace
  Cargo.lock                  # committed
  libwau/
    Cargo.toml
    src/
      lib.rs
      error.rs
      model/                  # Flavor, Channel, ProviderId, AddonId, etc.
      config/                 # config.toml types + XDG resolution
      manifest/               # manifest.toml schema + validation
      lock/                   # lockfile schema + read/write
      fs/                     # AddOns dir discovery, staging installs, removal, toc scan
      providers/              # Provider trait + implementations
      resolve/                # manifest -> resolution -> lock plan
      ops/                    # install/update/remove orchestration (library-level)
  wau/
    Cargo.toml
    src/
      main.rs                 # slim entrypoint: init logger, resolve settings, dispatch to app
      app/
        mod.rs                # main helpers and top-level dispatch
      cli/
        mod.rs                # clap definitions and argument parsing
      config/
        mod.rs                # config file loading and XDG path resolution
      logger/
        mod.rs                # tracing subscriber init and control
      output/
        mod.rs                # printing, tables, color decisions
      settings/
        mod.rs                # resolved runtime settings (cli > config merge)
  docs/
    WAU_RS_PLAN.md            # this plan
```

Crate boundary rules:

- `libwau` contains **no clap** and does not print.
- `wau` contains **no addon logic** beyond calling `libwau`.
- `cli` and `config` are consumed only inside `settings`; after `Settings` is constructed, only `Settings` is passed around — `cli` and `config` must not be used downstream.
- `main` is the minimal entrypoint: init logger, resolve settings, call `app`. Helpers live in `app/` (and optionally `utils/`).

### 2.0 Module + tests file policy (mandatory)

All modules across the workspace are directory modules with a sibling test file. **Tests must NEVER live in the same `.rs` file as logic.**

Example pattern (required):

```text
libwau/src/manifest/
  mod.rs
  tests.rs
```

And wiring inside `mod.rs`:

```rust
mod tests;
```

### 2.1 Toolchain and dependency policy

- **Rust edition**: `2024` across the workspace (align with current ecosystem direction).
- **Versioning**:
  - Only add the **newest** available version of a dependency at the time it is introduced.
  - In `Cargo.toml`, use **`x.y`** (preferred) or **`x`** version requirements (no `x.y.z`).
  - Exact versions are pinned by `Cargo.lock` (committed).
- **Dependency health**:
  - Only use **widely adopted** libraries.
  - Reject dependencies that are **obsolete**: repository is archived or has no meaningful activity (e.g. no commits) within ~**1 year** at the time it is added.
- **Async policy**:
  - network + downloads: async (`reqwest`)
  - filesystem + extraction: synchronous, but orchestrated safely (staging + atomic rename)

---

## 3. Data model

### 3.1 Config (`config.toml`)

Purpose: machine-local paths, defaults, and secrets.

Target location resolution:

- built-in defaults
- `$XDG_CONFIG_HOME/wau/config.toml` (fallback `~/.config/wau/config.toml`)
- `--config <path>` override

Includes:

- WoW root(s) or explicit flavor paths
- default flavor(s)
- cache dir
- provider credentials (e.g. CurseForge API token)

**Target shape (illustrative):**

```toml
[paths]
# Optional: explicit per-flavor install roots (preferred; avoids guessing).
retail = "/games/World of Warcraft/_retail_"
classic = "/games/World of Warcraft/_classic_"

# Where wau stores downloads, extracted staging dirs, and metadata.
cache = "~/.cache/wau"

[defaults]
flavor = "retail"
channel = "stable"

[providers.curseforge]
api_key = "..."
```

### 3.2 Manifest (`manifest.toml`)

Purpose: desired addons, provider selection, per-addon overrides. Shareable.

Includes:

- `[[addon]] name = "..."`
- `provider = "curse" | "github" | ...`
- provider-specific identifiers
- optional `channel`, optional pin constraints, optional flavor constraints

**Target shape (illustrative):**

```toml
schema = 1

[[addon]]
name = "Bagnon"
provider = "curseforge"
project_id = 12345
channel = "stable"            # optional override
flavors = ["retail"]          # optional restriction
pin = { version = "10.0.0" }  # optional; provider-specific pins also allowed (file_id/tag/sha)

[[addon]]
name = "SomeAddon"
provider = "github"
repo = "owner/repo"
asset_regex = "SomeAddon-.*\\.zip"
channel = "beta"
```

### 3.3 Lock (`lock.toml`)

Purpose: reproducibility + speed; records resolved artifacts and installed folders.

Includes per addon:

- resolved version/release id
- download URL + sha256 (when available)
- installed folder list
- timestamps, provider metadata

**Target shape (illustrative):**

```toml
schema = 1
generated_at = "2026-04-22T00:00:00Z"

[[addon]]
name = "Bagnon"
provider = "curseforge"
project_id = 12345
flavor = "retail"
channel = "stable"

# Resolution / reproducibility
resolved_version = "10.0.0"
resolved_id = "cf-file-999999"
download_url = "https://..."
sha256 = "..."

# Install bookkeeping
installed_dirs = ["Bagnon", "Bagnon_Config"]
installed_at = "2026-04-22T00:00:00Z"

# Optional: keep last-known-good resolutions to enable easy rollback/downgrade.
[[addon.history]]
resolved_version = "9.9.0"
resolved_id = "cf-file-888888"
download_url = "https://..."
sha256 = "..."
installed_at = "2026-03-01T00:00:00Z"
```

---

## 4. Provider abstraction

### 4.1 Provider trait (library)

Each provider must support, at minimum:

- search (optional for v0, but encouraged)
- resolve a manifest entry to a concrete downloadable artifact for a given flavor + channel
- download artifact to a path (or provide URL + checksum)

Providers must be written so new providers can be added without changing core orchestration logic.

### 4.1.1 Resolution rules (provider-agnostic)

Given a manifest entry + `(flavor, channel)`:

- Provider resolves to a **single** artifact (zip) plus identity metadata:
  - **provider-scoped ID** (file id / release tag / commit sha)
  - **version string** (human readable)
  - **download URL** (or a streaming body)
  - optional **checksum** (sha256)
  - optional “supported flavors/game versions” metadata used to avoid wrong installs
- Lockfile records the resolution exactly so re-running installs is deterministic.
- If the manifest specifies a **pin**:
  - Providers must attempt to resolve the pinned target; failing that, the operation errors with a clear message (no silent “closest match”).
- If the manifest changes **provider** for the same logical addon name:
  - Treat as a **provider switch**; resolve from the new provider; install flow runs normally.
  - Preserve settings backups and installed directory tracking across the switch (see §5.3).

### 4.2 v0 providers

Ship at least:

- **Local** provider (file path or URL): enables end-to-end install/update pipeline without external API dependencies.
- **CurseForge** provider skeleton: types + auth + minimal “resolve latest file” path; real install support can land incrementally.

Future (explicitly planned):

- GitHub Releases
- WowInterface
- Wago

---

## 5. Operations

- **Inventory**: scan installed addons from `Interface/AddOns` by reading `.toc` metadata; group folders belonging to the same addon.
- **Install**: resolve manifest → download → extract → determine top-level addon dirs → stage → swap into `AddOns` → update lock.
- **Update**: compare lock to provider latest for chosen channel; apply install flow; update lock.
- **Remove**: remove all directories recorded in lock (or discovered) for an addon; update lock.
- **List/status**: show installed addons, manifest addons, and update availability.
- **Backup/restore**: backup addon settings before destructive operations; allow restore from a chosen backup.
- **Downgrade/rollback**: install a previous version using manifest pin or lock history.

Safety rules:

- Always install into a staging directory under the target flavor, then rename into place.
- Keep a rollback directory for a single operation (optional for v0; required before 1.0).

### 5.1 Install semantics (zip handling)

- Treat provider artifacts as **zip** bundles by default.
- Extract into a **staging directory** under cache.
- Determine which top-level folders are addon directories (contains at least one `.toc`).
- Install by copying/renaming those directories into `Interface/AddOns` atomically.
- Record installed directory list into the lockfile; removal uses that list.

### 5.2 Inventory semantics (`.toc`)

Inventory is defined by scanning `Interface/AddOns/*/*.toc` and parsing at minimum:

- Title / name
- Interface version(s) or flavor tags (when present)
- Version (when present)
- Folder name (install directory)

Inventory is best-effort: `.toc` formats vary; parsing must be robust and non-panicking.

### 5.3 Settings backup semantics (SavedVariables)

- Back up settings from:
  - account-wide SavedVariables: `WTF/Account/<AccountName>/SavedVariables/*.lua`
  - character-specific SavedVariables (if present): `WTF/Account/<AccountName>/<Realm>/<Character>/SavedVariables/*.lua`
- Backups are stored per logical addon and timestamped.
- Default behavior for v0:
  - **On update**: create backup before replacing addon directories.
  - **On remove**: create backup before deletion.
  - Provide `--no-backup` to opt out per command; config controls defaults and retention.

### 5.4 Previous version install semantics

- Allow selecting a specific version via manifest `pin` or CLI flag.
- Prefer re-installing from lock history (already-known URL/checksum) when possible; otherwise resolve from provider for that version.
- If the selected version is unavailable from the current provider, fail clearly and suggest switching provider or unpinning.

---

## 6. CLI (paru-like)

### 6.0 wau module architecture

- **`cli/mod.rs`**: clap definitions and raw argument parsing only. No resolution logic.
- **`config/mod.rs`**: config file loading (XDG path resolution, TOML parse). No logic beyond deserialization.
- **`settings/mod.rs`**: merges `cli` args over `config` into a `Settings` struct. The only place `cli` and `config` are consumed. After construction, only `Settings` flows through the app.
- **`logger/mod.rs`**: tracing subscriber initialization and runtime control.
- **`output/mod.rs`**: printing helpers, tables, color decisions.
- **`app/mod.rs`**: main orchestration helpers; `main` delegates here. Utilities may also live in `utils/`.
- **`main.rs`**: as slim as possible — init logger, resolve settings, call `app`.

The CLI should feel like “a real package manager”:

- concise defaults
- short flags
- a “sync/update” command that can do `--refresh` + `--sysupgrade` style behavior

Target commands (names may evolve, but ergonomics are the goal):

- `wau sync` (install/update; analogous to `paru -S`)
  - `wau sync <addon...>` install
  - `wau sync --update` update from manifest/lock
  - `wau sync --manifest <path>` override
  - `wau sync --flavor <retail|classic...>`
  - `wau sync --channel <stable|beta|alpha|git>`
- `wau search <query>` (analogous to `paru -Ss`)
- `wau remove <addon...>` (analogous to `paru -R`)
- `wau list` (installed + manifest status; analogous to `paru -Q`)
- `wau info <addon>` (details; analogous to `paru -Qi`)

### 6.1 Paru-like command mapping (intent)

- **Sync/install/update**
  - `paru -S <pkg>` ⇢ `wau sync <addon>`
  - `paru -Syu` ⇢ `wau sync --refresh --update` (refresh provider caches + update)
  - `--noconfirm` ⇢ same meaning in `wau`
- **Search**
  - `paru -Ss <query>` ⇢ `wau search <query>`
- **Query installed**
  - `paru -Q` / `-Qi` ⇢ `wau list` / `wau info`
- **Remove**
  - `paru -R <pkg>` ⇢ `wau remove <addon>`

### 6.2 Additional commands/flags (for goals in §1.1)

- **Backup/restore**
  - `wau backup [addon...]` (create settings backup now)
  - `wau restore <addon> [--backup <id|timestamp>]`
- **Version selection**
  - `wau sync <addon> --version <...>` (install/downgrade)
  - `wau sync <addon> --rollback` (reinstall last-known-good from lock history)
- **Provider switching**
  - `wau sync <addon> --provider <...>` (override provider for one operation)
  - Manifest remains the canonical desired provider; CLI overrides are for one-off actions unless persisted.

---

## 7. Quality gates

Whenever a phase/step is marked complete, the following must pass:

- `cargo fmt --check`
- `typos`
- `cargo deny check licenses`
- `cargo clippy --workspace --all-targets --no-default-features -- -D warnings`
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- `cargo test --workspace --no-default-features`
- `cargo test --workspace --all-features`
- `cargo doc --workspace --no-deps`

### 7.1 Test discipline (geotiles/imgvwr rule)

- **Never inline tests with logic**. Unit tests live in sibling `tests.rs` files per module directory (see §2.0). Avoid inline `#[cfg(test)] mod tests { ... }` blocks inside implementation files.
- Integration tests live under `libwau/tests/` and are allowed to use fixtures (small zip files, small addon directory trees).
- Large external assets (if ever needed) must not be committed; use a cached download pattern (geotiles).

### 7.2 CI blueprint (tofi/imgvwr-style)

The goal is a predictable set of workflows, with feature matrices, cached builds, and strict linting:

- `build.yml`: build matrix (default features, `--all-features`, `--no-default-features` if applicable)
- `fmt-clippy.yml`: `cargo fmt --check`; `cargo clippy -D warnings` (same matrix)
- `test.yml`: `cargo test --workspace`
- `doc.yml`: `cargo doc --workspace --no-deps` (ensure docs build on CI)
- `typos.yml`: spelling
- `deny.yml`: `cargo deny check licenses`
- `deploy.yml`: optional; release artifacts for `wau` binary
- `dependabot.yml`: cargo + github-actions

---

## 8. Phased steps

### Phase 0 — Workspace + hygiene

- [x] Restructure workspace to match §2
- [x] Add error types (`thiserror`) + `tracing` foundation
- [x] Replace placeholder CI with matrixed build/fmt/clippy/test + typos + deny + dependabot

**Verify**: all gates in §7 pass on a clean tree.

### Phase 1 — Core model + config/manifest/lock schemas

- [ ] Implement `Flavor`, `Channel`, provider ids, and typed keys
- [ ] Implement XDG config resolution and parse `config.toml`
- [ ] Implement `manifest.toml` and `lock.toml` read/write + validation

**Verify**: unit tests for parse/merge/validation.

### Phase 2 — Addon inventory

- [ ] `.toc` parser (minimal but robust)
- [ ] `Interface/AddOns` scanner and grouping
- [ ] `wau list` shows installed addons

**Verify**: tests with fixture addon folders + `.toc` samples.

### Phase 3 — Install/remove pipeline (Local provider first)

- [ ] Download/extract zip
- [ ] Staging install + atomic swap
- [ ] Lock updates
- [ ] `wau sync <path-or-url>` and `wau remove`

**Verify**: integration tests with local zip fixtures.

### Phase 4 — Providers + resolution engine

- [ ] Provider trait and registry
- [ ] CurseForge minimal “resolve latest file for flavor+channel” path (auth via token)
- [ ] Manifest -> resolution -> plan engine

**Verify**: unit tests with mocked provider; manual smoke test against CurseForge.

### Phase 5 — Paru-like UX polish

- [ ] interactive confirmation prompts (opt-out `--noconfirm`)
- [ ] `--quiet`, `--verbose`, consistent exit codes
- [ ] shell completions (optional; similar to tofi-rs approach)

### Phase 6 — Release discipline + cleanup (tofi-rs style)

- [ ] Ensure `main` does not carry migration-era scaffolding (temporary notes, “TODO diaries”, stale docs)
- [ ] Ensure docs point to the current behavior and current schema versions
- [ ] Tag a v0 release and document compatibility expectations

---

## 9. Definition of done (v0)

- [ ] `wau list` prints installed addons for a selected flavor
- [ ] `wau sync` installs addons from `manifest.toml` using at least one working provider
- [ ] `wau sync --update` updates according to channel rules
- [ ] `wau remove` removes installed addon dirs safely
- [ ] Lockfile correctly tracks installed dirs and resolved artifact identity
- [ ] CI green on pushes/PRs
- [ ] Linux is supported and documented (paths, file permissions, and default XDG locations)

---

## 10. Post-v0 roadmap (toward 1.0)

- **Provider expansion**: GitHub Releases, WowInterface, Wago; define stable provider IDs and normalization rules.
- **Platform expansion**: add Windows + macOS support (paths, extraction quirks, filesystem semantics, and CI coverage).
- **Lock stability**: stable lock schema with forward-compatible parsing; ensure upgrades preserve data.
- **Rollback**: implement a real rollback strategy for install/update failures (required before 1.0).
- **Performance**: cache provider metadata and downloads; avoid redundant unzip/work when lock indicates no change.

---

## 11. Document maintenance

Update this plan whenever:

- a policy decision changes (schemas, install semantics, provider trait)
- phases are reordered or new ones are added
- CI/tooling layout changes

### Revision history

| Date       | Change                                                                                                            |
| ---------- | ----------------------------------------------------------------------------------------------------------------- |
| 2026-04-22 | Initial plan created for `wau` rewrite                                                                            |
| 2026-04-22 | Expanded into a playbook: schema examples, CI blueprint, test discipline, paru mapping, and release hygiene phase |
