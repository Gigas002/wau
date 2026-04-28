# wau CLI

Target ergonomics: **paru-like** — terse defaults, short flags, predictable exit codes, useful `--help`, non-interactive mode.

Maintainers: treat this file as the **command surface reference**; the plan in `docs/WAU_RS_PLAN.md` references it so CLI debates do not bloat the roadmap.

---

## Crate layout (implementation)

- `wau/src/cli/mod.rs` — clap definitions only; no resolution logic.
- `wau/src/config/mod.rs` — config load + deserialize.
- `wau/src/settings/mod.rs` — merge CLI over config; downstream code sees only `Settings`.
- `wau/src/app/mod.rs` — orchestration; `main` stays thin.

---

## Commands

### Sync / install / update (`paru -S` family)

- `wau sync` — install or update all addons from manifest.
- `wau sync --update` — apply updates (re-resolve latest for each addon).
- `wau sync --manifest <path>` — manifest path override.
- `wau sync --tag <tag>` — target a configured install tag (see `examples/config.toml`).
- `wau sync --flavor <…>` — flavor override without changing the config default.
- `wau sync --channel <stable|beta|alpha>` — channel override without changing the config default.

### Search

- `wau search <query>` — local search across manifest and installed addons.

### Remove

- `wau remove <addon…>` — `paru -R`-style removal using lock + safe dir list.

### Query

- `wau list` — installed addons for the active install tag.
- `wau info <addon>` — detail view: manifest entry + lock state.

### Init

- `wau init` — create `manifest.toml` and `<tag>.lock.toml` in the config directory.
- `wau init --tag <tag>` — target a specific install tag's lock file.
- `wau init --manifest <path>` — override the manifest output path.
- `wau init --force` — overwrite existing files.

### Global flags

- `--noconfirm` — skip interactive confirmation prompts.
- `-q` / `--quiet` — suppress per-item progress output.
- `-v` / `--verbose` — enable debug logging.
- `--config <path>` — config file override.
- Log verbosity priority (highest wins): `RUST_LOG` env > `--verbose` > `--quiet` > `[logging].level` in config.

---

## Deferred / late-phase commands (not required for first release)

These stay in design docs until the core pipeline and providers are stable:

- **Named-addon sync**: `wau sync <addon…>` — install or update specific addons from the manifest (resolution rules deferred).
- **Cache refresh**: `wau sync --refresh --update` — refresh provider caches, then update (sketch: `paru -Syu`).
- **Backup / restore** (SavedVariables): `wau backup`, `wau restore`, retention flags.
- **Specific version / non-latest**: manifest **`pin`** fields, `wau sync <addon> --version …`, `wau sync <addon> --rollback`, rich lock `history` — **first release installs and updates latest only** (see `docs/WAU_RS_PLAN.md` §1.1, §4.1.1, Phase 8).
- **One-off provider override on CLI**: useful for experiments; **not** "provider switching" as a first-class migration story (see plan: cross-provider identity is unsolved).
- **Shell completions** (**bash**, **zsh**, **fish**, **Nushell**) — **Phase 10** in `docs/WAU_RS_PLAN.md`.

---

## Paru mapping (quick reference)

| paru              | wau                          |
| ----------------- | ---------------------------- |
| `paru -S <pkg>`   | `wau sync` (manifest-driven) |
| `paru -Syu`       | `wau sync --update`          |
| `paru -Ss <q>`    | `wau search <q>`             |
| `paru -Q` / `-Qi` | `wau list` / `wau info`      |
| `paru -R`         | `wau remove`                 |
| `--noconfirm`     | `--noconfirm`                |
