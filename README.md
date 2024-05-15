# wau

Wow Addon Updater

## (planned) features

- lightweight, pretty, native and fast cli, alike `yay`/`paru`
- search, install, remove or update your addons from multiple providers (`curse`, `wowinterface`, `github`, etc), for multiple flavors (`retail`, `classic`, custom servers, etc), and multiple release channels (`release`, `beta`, `git`, etc)
- easily tweak and share your addons installation configuration with `manifest.toml`

## configuration

see the repo's `config.toml` and `manifest.toml` files for example with default values and explanations

## usage

running `wau` to install or update your addons **requires** first generating the `manifest.toml` file, or copying it to default location (`~/.config/wau/manifest.toml`)

### manifest

`wau manifest` -- generates `manifest.toml` file. By default, uses `config.toml` values as arguments

optional args:
- `--output %path%` -- path to your output `manifest.toml` file. By default will write or override one into `~/.config/wau/manifest.toml`
- `--wow %path%` -- path to your wow installation. If wow root provided - flavor decided automatically. If flavor provided - specify flavor manually

sub-commands:
- `install %path%` -- install addons from provided `manifest.toml` file

### update

`wau`/`wau update` -- updates (or installs non-existent) addons using `manifest.toml` data

### search

`wau %addon_name%`/`wau search %addon_name%` -- search for addon on supported providers

### install

`wau install %addon_name%` -- install addon

### remove

`wau remove %addon_name%` -- removes addon

## roadmap

- [ ] [basic features] properly recognize installed addons (`toc` parser)
- [ ] [basic features] `manifest.toml` specs and generation
- [ ] [basic features] cli `config.toml` specs
- [ ] [basic features] cli commands and arguments
- [ ] [repo organization] implement ci/cd through github actions
- [ ] [provider support] `curse` -- `retail` flavor, `release` channel
- [ ] [provider support] `curse` -- all official flavors, all release channels
- [ ] other providers support

## thanks to

- [ajour](https://github.com/ajour/ajour), some implementations are inhereted from there
- [instawow](https://github.com/layday/instawow)
- [wowup](https://github.com/WowUp)
