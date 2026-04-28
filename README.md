# wau

World of Warcraft Addon Updater — a fast, paru-like CLI for managing WoW addons across multiple installs, flavors, and release channels.

## Features

- **Multi-provider**: install and update addons from CurseForge, WoWInterface, GitHub, and local zips
- **Multi-flavor**: retail, classic-era, classic-cata, and all official Blizzard flavors; custom installs (private servers) work via user-defined install tags
- **Multi-channel**: stable, beta, alpha — per-addon overrides supported where providers expose them
- **Manifest-driven**: describe desired addons in `manifest.toml`; `lock.toml` records exact resolved versions for reproducibility
- **Safe installs**: staging + atomic swap; never corrupts `Interface/AddOns` on partial failure
- **Paru-like UX**: terse defaults, `--noconfirm`, `--quiet`/`--verbose`, consistent exit codes

## Installation

```sh
cargo install --path wau
```

## Usage

```sh
# Install all addons in manifest.toml
wau sync

# Update to latest releases
wau sync --update

# List installed addons
wau list

# Show detail for one addon
wau info WeakAuras

# Search manifest and installed addons
wau search aura

# Remove addons
wau remove WeakAuras Plater

# Use a specific install tag (multiple WoW roots)
wau sync --tag classic-official
```

See `examples/cli.md` for the full command reference and `examples/config.toml` for configuration options.

## Configuration

`wau` reads `$XDG_CONFIG_HOME/wau/config.toml` (fallback `~/.config/wau/config.toml`). Override with `--config <path>`.

Minimal config:

```toml
schema = 1

[defaults]
flavor = "retail"
channel = "stable"
install_tag = "main"

[paths]
cache = "~/.cache/wau"

[[paths.installs]]
tag = "main"
flavor = "retail"
wow_root = "/path/to/World of Warcraft"
```

See `examples/config.toml` for the full schema including multiple install roots, logging level, and provider credentials.

## Providers

| Provider    | Requires                    | Feature flag   |
| ----------- | --------------------------- | -------------- |
| CurseForge  | API key (free at console.curseforge.com) | `curseforge` |
| WoWInterface | None                       | `wowinterface` |
| GitHub      | None (token for higher rate limit) | `github` |
| Local       | None                        | `local`        |

All providers are enabled by default. Build with `--no-default-features --features <list>` for a minimal binary.

## Workspace layout

```
wau/
  libwau/   — domain logic (model, providers, install/remove ops, manifest/lock)
  wau/      — CLI wrapper (clap, settings resolution, output formatting)
```

`libwau` has no clap dependency and does not print; all presentation lives in `wau`.

## Acknowledgements

- [ajour](https://github.com/ajour/ajour) — provider semantics and install edge-case reference
- [instawow](https://github.com/layday/instawow) — manifest/lock "desired vs resolved state" approach
- [paru](https://github.com/morganamilo/paru) — CLI ergonomics inspiration

## License

GPL-3.0-only
