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

**Example artifacts (schemas + CLI sketch)** live under `examples/` so this plan stays readable and reviewers can diff concrete shapes without embedding TOML here:

- `examples/config.toml` — paths, defaults, **install tags**, **`[logging]`**, provider secrets
- `examples/manifest.toml` — desired addons, channels, flavor constraints; **pin** shapes are **forward-looking** (late phase), see §1.1.1
- `examples/lock.toml` — resolved artifacts + install bookkeeping
- `examples/cli.md` — command names, flags, paru mapping, deferred commands

---

## 1. Goals and constraints

### 1.1 Goals

- **Two-crate workspace**:
  - **`libwau`**: addon domain model, manifest/lock/config resolution, provider abstraction, filesystem operations (install/update/remove), and tests.
  - **`wau`**: thin CLI wrapper (arg parsing, printing, interactive prompts).
- **Multi-provider**: multiple addon sources behind a **stable provider trait** and registry. Providers are pluggable; **first release** ships a **small, high-value set** (see §4.2); more sources are added in **post-release** phases after surveying what players actually use.
- **Multi-flavor + install disambiguation**:
  - Support Blizzard-style **flavor ids** (retail, classic-era, classic-cata, …) for resolution metadata.
  - Support **multiple physical installs** that share the same flavor id (private / “pirate” servers, duplicate clients, experimental trees) via a user-defined **`install_tag`** (or equivalent) in config so manifests/locks/commands target the intended tree without ambiguity.
- **Community and private ecosystems**:
  - **Killer feature (architecture)**: custom realms and their addon ecosystems must be supportable **without forking** `libwau`: third-party **provider plugins** (or in-process registration from wrapper binaries) and **custom flavor / channel semantics** where the core only treats them as opaque ids + paths passed to providers.
  - **Not** a promise to ship every exotic host out of the box — a promise that the **crate boundaries** (provider trait, config-driven install roots, flavor/tag typing) do not hard-code “only official Blizzard.”
- **Multi-channel**: stable / beta / alpha / git-style streams and per-addon overrides where providers support them.
- **Own manifest + package management**:
  - **Manifest** defines _desired state_ (what addons, which provider, channel, flavor constraints; **pinning / specific versions are late**, see §1.1.1).
  - **Lock** records _resolved state_ (exact release/build, download URL, checksums, installed folders, timestamps).
- **First-release resolution rule**: **install and update only ever fetch the single “latest” artifact** the provider returns for that manifest row’s identifiers + **channel** + install flavor context. There is **no** “install this semver / this file id / this commit” behavior in the resolver until the late **version-selection** work (§1.1.1, Phase 8). Schemas may still **parse** optional `pin` fields for forward compatibility, but the engine treats them as **unsupported** (reject manifest with pins, or ignore with a loud warning—pick one policy in implementation docs).
- **Paru-like CLI design**: terse defaults, clear subcommands and short flags, consistent exit codes, useful `--help`, optional color, non-interactive mode. **Concrete command drafts**: `examples/cli.md`.
- **Safe installs**:
  - Never corrupt `Interface/AddOns` on partial failure.
  - Staging installs in a temp dir; only “swap in” on success.
  - Track installed directories per addon.
- **Platform support**:
  - **v0 target**: Linux.
  - Post-v0: Windows and macOS.

#### 1.1.1 Explicitly phased later (not required to ship the first pipeline)

The **data model and file schemas** should still **reserve** optional fields / extension points (e.g. lock history tables, backup metadata hooks, manifest `pin` tables) so these features land without invasive refactors:

- **SavedVariables backup / restore** automation, retention policies, and CLI around them.
- **Specific-version installation** of any kind: manifest **`pin`** (semver, `file_id`, tag, commit, sha, …), **git ref pinning** beyond “whatever latest means for that provider”, CLI **`--version` / `--rollback`**, and **rollback / downgrade UX** (rich lock history, reinstall older resolution, interactive pickers). **First release** remains **latest-only** (§1.1, §4.1.1).
- **Shell completions** for **bash**, **zsh**, **fish**, and **Nushell** (generate or ship completion scripts; document install paths per shell). Tracked as **Phase 10** — not required for the first UX-polish milestone (Phase 5).

These are **late roadmap steps** with their own verification stories once core sync is stable.

#### 1.1.2 Provider switching — out of scope for first release (design only)

**User-visible “migrate this addon from provider A to provider B while preserving identity”** is **deferred**: it couples install layout, settings, and **cross-provider identity**, which we **cannot** infer reliably today.

- There is **no built-in, queryable, trustworthy database** mapping “CurseForge project X ≡ GitHub repo Y ≡ WowInterface file Z.” Without that, “switch provider for the same logical addon” is either **manual** (user edits manifest and accepts a new identity) or **heuristic** (fragile).
- **Research note**: investigate whether projects like **[instawow](https://github.com/layday/instawow)** (or others) maintain an **open**, reusable crosswalk. If a community-maintained mapping exists and is license-compatible, **future** automation becomes realistic; if not, switching remains an **explicit manifest edit** and reinstall, not a magic core feature.
- **Engineering requirement**: still design `libwau` so a **future** “switch” or “alias” layer could exist (stable internal ids, provider-agnostic install records, clear separation between “manifest row” and “installed artifact record”) — but **do not** implement migration orchestration in early phases.

### 1.2 Discipline (non-negotiable)

- **Library-first**: all business logic and I/O is in `libwau`; `wau` is a thin presentation/wiring layer.
- **Step sizing**: follow "small, verifiable steps" (imgvwr/geotiles) rather than big-bang refactors.
- **Release hygiene**: keep `main` clean of "planning diary noise" and migration-era scaffolding (tofi-rs). Plans live in `docs/`, and historical plans can be moved to a branch/tag when stable.
- **Stay slim**: keep each module, function, and file focused and minimal. Prefer many small, clear units over few large ones.
- **Naming**: short but descriptive names; optimize for the reader, not the writer.

### 1.3 Non-goals

- GUI, background daemon, tray integration.
- Full “catalog browser” UI; CLI only.
- Syncing addons across machines automatically (can be layered later via manifest + lock).
- **First-class automated provider switching** in early releases (see §1.1.2).
- **Non-latest installs** in early releases: no provider-backed “install exactly this version / this file / this commit” in the **resolution engine** (see §1.1.1).
- Supporting **every** provider immediately: architecture must allow addition; **first release** focuses on **CurseForge + WoWInterface + GitHub** (plus a **Local** path/URL provider for tests and power users). Additional hosts are **post-release** work after demand research.

### 1.4 Definitions

- **Addon**: an installed package in `Interface/AddOns/` (usually multiple folders) described by one or more `.toc` files.
- **Provider**: remote index + artifact distribution service (CurseForge, GitHub, WowInterface, …) implementing the provider trait.
- **Flavor**: logical WoW product line used in resolution metadata (e.g. retail vs classic-era vs classic-cata).
- **Install tag**: user-defined label in config identifying **one** WoW root + addons tree; disambiguates multiple installs (same or different flavor ids). See `examples/config.toml`.
- **Channel**: release stream preference (stable/beta/alpha/git-style) interpreted per provider.

### 1.5 Compatibility targets (behavioral reference only)

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
  examples/                   # illustrative schemas + CLI draft (not embedded in this plan)
    config.toml
    manifest.toml
    lock.toml
    cli.md
  libwau/
    Cargo.toml
    src/
      lib.rs
      error.rs
      model/                  # Flavor, Channel, ProviderId, AddonId, InstallTag, etc.
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

### 2.2 Provider feature gating (mandatory)

Providers differ wildly in **protocols, credentials, optional system tools, and transitive deps** (e.g. a git-snapshot provider may shell out to **`git`** or link VCS libraries; another may pull a heavy SDK). To keep **minimal builds**, **predictable CI**, and **clear optional dependencies**, the following rules apply:

- **Every concrete provider** in `libwau` lives behind its **own Cargo `feature`** (one feature per provider, or per small provider group if truly inseparable — document the exception). No provider-specific dependencies compile into the default graph unless that feature is enabled.
- **`wau`** passes through the same feature flags to `libwau` (thin `wau` features that enable `libwau` features). Downstream packagers can ship `wau` with a subset of providers.
- **Registry / trait wiring** in `libwau` must remain **buildable** with `--no-default-features` (core types + empty or Local-only registry, as defined for that mode).
- **Community or third-party providers** follow the same rule: **separate feature** and/or **separate workspace crate** that depends on `libwau` and registers through a single supported extension path — never “always on” hidden imports.
- **CI** (see §7.2) must keep exercising **default features**, **`--no-default-features`**, and **`--all-features`** so optional provider deps do not bitrot.

Rationale: avoids forcing every user to pay for Curse + Git + WowInterface + future hosts; documents “this binary was built without provider X” at compile time where possible.

---

## 3. Data model

Illustrative **TOML shapes** are maintained in `examples/*.toml` so contributors can review and propose edits without bloating this document. The plan only states **intent** and **invariants**.

### 3.1 Config (`config.toml`)

Purpose: machine-local paths, defaults, **install tags** (multiple roots / private servers), **default log level**, and secrets.

Target location resolution:

- built-in defaults
- `$XDG_CONFIG_HOME/wau/config.toml` (fallback `~/.config/wau/config.toml`)
- `--config <path>` override

Includes (see `examples/config.toml`):

- one or more **tagged** WoW install roots + associated flavor id
- default install tag / flavor / channel for commands
- cache dir
- **`[logging].level`** — default tracing verbosity when CLI does not override (`trace` / `debug` / `info` / `warn` / `error`; exact allowed set in schema)
- provider credentials (e.g. CurseForge API token)

### 3.2 Manifest (`manifest.toml`)

Purpose: desired addons, provider selection, per-addon overrides. Shareable.

Includes (see `examples/manifest.toml`):

- repeated addon tables with `name`, `provider`, provider-specific ids
- optional `channel`, flavor constraints
- **First release**: rows are interpreted as “this addon from this provider/channel for these flavors”; resolution is **latest-only** (§4.1.1).
- **Late / forward-looking in examples**: `pin`, git-ref modes, duplicate-name edge cases — types may parse them, **resolver must not** honor them until Phase 8.

### 3.3 Lock (`lock.toml`)

Purpose: reproducibility + speed; records resolved artifacts and installed folders.

Includes per addon (see `examples/lock.toml`):

- resolved version/release id, download URL, sha256 when available
- installed folder list, timestamps, provider metadata
- optional **history** sections (shaped in examples, implemented in late phases for rollback UX)

---

## 4. Provider abstraction

### 4.1 Provider trait (library)

Each provider must support, at minimum:

- search (optional for early releases, but encouraged)
- **resolve latest** — given a manifest row + `(install context → flavor metadata, channel)`, return the **single current “latest”** artifact that provider defines for that channel (no pin arguments in **first release**)
- download artifact to a path (or provide URL + checksum)

Providers must be written so **new providers can be added without changing core orchestration logic** — including **community-maintained** providers for private servers, as long as they implement the trait and are registered behind **Cargo features** per §2.2 (static registration in code is fine; **no** always-on provider deps).

### 4.1.1 Resolution rules (provider-agnostic)

**First release — latest only**

Given a manifest entry + install context (tag → paths + flavor id) + channel:

- Provider resolves to a **single** artifact (zip or archive) — the **latest** offering for that addon identity and channel the provider exposes — plus identity metadata:
  - **provider-scoped ID** (file id / release tag / commit sha, …)
  - **version string** (human readable)
  - **download URL** (or a streaming body)
  - optional **checksum** (sha256)
  - optional “supported flavors/game versions” metadata used to avoid wrong installs
- Lockfile records whatever was installed so the next run can detect **“still latest vs newer available”** for updates; **reproducible non-latest installs** are a **late** concern (Phase 8).

**Late phase — pins and specific versions**

- If the manifest specifies a **pin** (or CLI requests a version): providers resolve that exact target; failing that, error clearly (no silent “closest match”). Implemented together with Phase 8 / manifest policy.

**Future** (if provider switching / aliases exist): any migration rules must be explicit and identity-backed — not implied from addon display names.

### 4.2 First-release vs post-release providers

**First public release (target set)** — ship working, tested integrations for:

- **CurseForge**
- **WoWInterface**
- **GitHub** (releases + git-archive style as sketched in examples)
- **Local** (path or `file://` / HTTP URL to a zip): end-to-end tests and power-user workflows without catalog APIs

**Post-release (research-driven)**:

- Survey usage (forums, Discord, existing launchers) and add providers such as **Wago**, other CDNs, or community indexes — each behind the same trait + registry.
- Re-evaluate **provider switching** only if a **maintainable cross-provider identity story** exists (§1.1.2).

---

## 5. Operations

Core loop (early phases):

- **Inventory**: scan installed addons from `Interface/AddOns` by reading `.toc` metadata; group folders belonging to the same addon.
- **Install**: resolve manifest → download → extract → determine top-level addon dirs → stage → swap into `AddOns` → update lock.
- **Update**: compare lock to provider latest for chosen channel; apply install flow; update lock.
- **Remove**: remove all directories recorded in lock (or discovered) for an addon; update lock.
- **List/status**: show installed addons, manifest addons, and update availability.

**Late phases** (see §1.1.1): backup/restore, downgrade/rollback UX, richer history — keep types and file layout **forward-compatible** from day one.

Safety rules:

- Always install into a staging directory under the target install tag’s addons path, then rename into place.
- Keep a rollback directory for a single operation (optional for v0; tighten before 1.0).

### 5.1 Install semantics (zip handling)

- Treat provider artifacts as **zip** bundles by default (or provider-defined archive format with explicit support).
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

### 5.3 Settings backup semantics (SavedVariables) — late phase

Deferred feature set; when implemented:

- Back up settings from account-wide and character-specific `SavedVariables` trees.
- Timestamped backups per logical addon; configurable retention and `--no-backup` escapes.

### 5.4 Previous version install semantics — late phase (Phase 8)

Deferred feature set; when implemented:

- Prefer re-installing from lock history when URLs/checksums are still valid.
- Honor CLI and manifest pins as drafted in `examples/cli.md` and `examples/manifest.toml`.

---

## 6. CLI (paru-like)

**Draft command surface, flags, paru mapping, and explicitly deferred commands** live in:

- `examples/cli.md`

This section only fixes **architecture rules** (see §2 and the top of `examples/cli.md`): clap stays in `wau`, logic in `libwau`, `Settings` is the merged view.

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

- [ ] Implement `Flavor`, `Channel`, provider ids, **install tag** / multi-root config model, and typed keys
- [ ] Implement XDG config resolution and parse `config.toml` (paths, defaults, **`[logging].level`**, provider tables; keep forward-compatible optional sections)
- [ ] Implement `manifest.toml` and `lock.toml` read/write + validation aligned with `examples/*.toml` (optional `pin` keys may parse for forward compat; **reject or warn** if pins present until Phase 8 — document the chosen policy)

**Verify**: unit tests for parse/merge/validation.

### Phase 2 — Addon inventory

- [ ] `.toc` parser (minimal but robust)
- [ ] `Interface/AddOns` scanner and grouping
- [ ] `wau list` shows installed addons for a selected install tag / flavor

**Verify**: tests with fixture addon folders + `.toc` samples.

### Phase 3 — Install/remove pipeline (Local provider first)

- [ ] Download/extract zip
- [ ] Staging install + atomic swap
- [ ] Lock updates
- [ ] `wau sync` from local path/URL and `wau remove`

**Verify**: integration tests with local zip fixtures.

### Phase 4 — First-release providers + resolution engine

- [ ] Provider trait and registry; **each provider behind a Cargo `feature`** in `libwau` with matching passthrough in `wau` (§2.2)
- [ ] **CurseForge**: auth + **resolve latest** for flavor+channel + download
- [ ] **WoWInterface**: **resolve latest** + download path
- [ ] **GitHub**: **resolve latest** matching row (releases asset regex and/or git-ref “tip” mode per examples) — **no** honoring `pin` / non-tip refs until Phase 8
- [ ] Manifest → resolution → lock plan engine (**latest-only** semantics)

**Verify**: unit tests with mocked HTTP; smoke tests per provider as feasible in CI (may be manual + documented for keys).

### Phase 5 — Paru-like UX polish

- [ ] Commands per `examples/cli.md` (iterate the file as reality diverges)
- [ ] interactive confirmation prompts (opt-out `--noconfirm`)
- [ ] `--quiet`, `--verbose`, consistent exit codes

### Phase 6 — Release discipline + cleanup (tofi-rs style)

- [ ] Ensure `main` does not carry migration-era scaffolding (temporary notes, “TODO diaries”, stale docs)
- [ ] Ensure docs and `examples/` reflect current schema versions
- [ ] Tag a v0 release and document compatibility expectations

### Phase 7 — Backup / restore + retention (late)

- [ ] SavedVariables backup on destructive ops (opt-out flags)
- [ ] `wau backup` / `wau restore` as in `examples/cli.md` once stabilized

**Verify**: integration tests with synthetic `WTF` trees.

### Phase 8 — Pins, specific versions, history, downgrade / rollback (late)

- [ ] Honor manifest **`pin`** and equivalent provider-specific selection (semver, `file_id`, tag, commit, …)
- [ ] Lock **history** sections + CLI flows (`--version`, `--rollback`) as drafted in examples
- [ ] GitHub git-ref modes that are not “track tip” / latest-only, if still desired

**Verify**: fixture-driven tests for non-latest resolution and reinstall from history.

### Phase 9 — Provider switching / cross-provider identity (future, conditional)

- [ ] Only revisit after §1.1.2 research; may remain “manual manifest edit only” permanently if no trustworthy mapping exists.

### Phase 10 — Shell completions (late)

- [ ] **bash** completion (`wau` subcommands, flags, static values where feasible)
- [ ] **zsh** completion (same surface; `_wau`-style script or generated equivalent)
- [ ] **fish** completion
- [ ] **Nushell** completion (export or hand-maintained completions file per nu conventions)
- [ ] Document installation (system package vs `wau completion <shell>`-style generator — pick one approach and test on Linux first)

**Verify**: manual smoke in each shell; optional CI smoke only where cheap (e.g. bash `-n` / `compgen` checks).

---

## 9. Definition of done (v0)

- [ ] `wau list` prints installed addons for a selected install tag / flavor
- [ ] `wau sync` installs addons from `manifest.toml` using **Local** and at least **one** remote provider end-to-end (expand toward full first-release set in Phase 4); **only latest** provider resolution, **no** pin / specific-version paths
- [ ] `wau sync --update` updates to **latest** per channel rules (same constraint)
- [ ] `wau remove` removes installed addon dirs safely
- [ ] Lockfile correctly tracks installed dirs and resolved artifact identity
- [ ] CI green on pushes/PRs
- [ ] Linux is supported and documented (paths, file permissions, and default XDG locations)

---

## 10. Post-v0 roadmap (toward 1.0)

- **Provider expansion (post-release)**: additional hosts by demand (e.g. Wago); stable provider IDs and normalization rules in `examples/manifest.toml` + docs.
- **Private / custom servers**: document a **cookbook** pattern: custom `install_tag`, flavor id strategy, and a small custom provider crate or module.
- **Platform expansion**: Windows + macOS (paths, extraction quirks, CI).
- **Lock stability**: forward-compatible parsing; migrations preserve data.
- **Rollback**: operational rollback for failed install/update (required before 1.0).
- **Performance**: metadata and download caches; skip redundant work when lock shows no change.
- **Shell completions**: bash / zsh / fish / Nushell (see **Phase 10**).

---

## 11. Document maintenance

Update this plan whenever:

- a policy decision changes (schemas, install semantics, provider trait)
- phases are reordered or new ones are added
- CI/tooling layout changes

When **example shapes** change, update `examples/*.toml` / `examples/cli.md` first (or in the same PR), then adjust prose here if invariants moved.

### Revision history

| Date       | Change |
| ---------- | ------ |
| 2026-04-22 | Initial plan created for `wau` rewrite |
| 2026-04-22 | Expanded into a playbook: schema examples, CI blueprint, test discipline, paru mapping, and release hygiene phase |
| 2026-04-23 | Re-scoped first release providers (Curse + WoWInterface + GitHub + Local); **latest-only** resolution (pins → Phase 8); deferred backup/switching; install tags + private-server extensibility; moved TOML/CLI drafts to `examples/`; late phases 7–10; **§2.2** per-provider Cargo features; **`[logging].level`** in config + §3.1 |
