# wau CLI (illustrative)

Target ergonomics: **paru-like** — terse defaults, short flags, predictable exit codes, useful `--help`, optional color, non-interactive mode.

Maintainers: treat this file as the **draft command surface**; the plan in `docs/WAU_RS_PLAN.md` references it so CLI debates do not bloat the roadmap.

---

## Crate layout (implementation)

- `wau/src/cli/mod.rs` — clap definitions only; no resolution logic.
- `wau/src/config/mod.rs` — config load + deserialize.
- `wau/src/settings/mod.rs` — merge CLI over config; downstream code sees only `Settings`.
- `wau/src/app/mod.rs` — orchestration; `main` stays thin.

---

## Command intent (names may evolve)

### Sync / install / update (`paru -S` family)

- `wau sync` — install or update from manifest + lock.
- `wau sync <addon…>` — install named addons (resolution rules TBD).
- `wau sync --update` — apply updates per manifest/lock/channel.
- `wau sync --refresh --update` — refresh provider caches, then update (sketch: `paru -Syu`).
- `wau sync --manifest <path>` — manifest path override.
- `wau sync --install <tag>` / `wau sync --tag <tag>` — target a configured install tag (see `examples/config.toml`).
- `wau sync --flavor <…>` — flavor override when useful without switching default install.
- `wau sync --channel <stable|beta|alpha|…>` — channel override.

### Search

- `wau search <query>` — `paru -Ss`-style provider search (quality depends on provider).

### Remove

- `wau remove <addon…>` — `paru -R`-style removal using lock + safe dir list.

### Query

- `wau list` — installed + manifest alignment / update hints (`paru -Q`).
- `wau info <addon>` — detail view (`paru -Qi`).

### Global flags (sketch)

- `--noconfirm` — non-interactive confirmations.
- `--quiet` / `--verbose` — output level.
- `--config <path>` — config file override.
- Default log filter from `examples/config.toml` **`[logging].level`**; optional `--log-level` / `RUST_LOG` override (merge order TBD in implementation).

---

## Deferred / late-phase commands (not required for first release)

These stay in design docs until the core pipeline and providers are stable:

- **Backup / restore** (SavedVariables): `wau backup`, `wau restore`, retention flags.
- **Specific version / non-latest**: manifest **`pin`** fields, `wau sync <addon> --version …`, `wau sync <addon> --rollback`, rich lock `history` — **first release installs and updates latest only** (see `docs/WAU_RS_PLAN.md` §1.1, §4.1.1, Phase 8).
- **One-off provider override on CLI**: useful for experiments; **not** “provider switching” as a first-class migration story (see plan: cross-provider identity is unsolved).
- **Shell completions** (**bash**, **zsh**, **fish**, **Nushell**) — **Phase 10** in `docs/WAU_RS_PLAN.md` (not part of first UX-polish ship).

---

## Paru mapping (quick reference)

| paru              | wau (illustrative)        |
| ----------------- | ------------------------- |
| `paru -S <pkg>`   | `wau sync <addon>`        |
| `paru -Syu`       | `wau sync --refresh --update` |
| `paru -Ss <q>`    | `wau search <q>`          |
| `paru -Q` / `-Qi` | `wau list` / `wau info`   |
| `paru -R`         | `wau remove`              |
| `--noconfirm`     | `--noconfirm`             |
