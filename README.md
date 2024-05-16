# wau

Wow Addon Updater

## (planned) features

- lightweight, pretty, native and fast cli, alike `yay`/`paru`
- search, install, remove or update your addons from multiple providers (`curse`, `wowinterface`, `github`, etc), for multiple flavors (`retail`, `classic`, custom servers, etc), and multiple release channels (`release`, `beta`, `git`, etc)
- easily tweak and share your addons installation configuration with `manifest.toml`

## configuration

see the repo's `config.toml` and `manifest.toml` files for example with default values and explanations. By default, config file doesn't exist, so you'll have to create it and pass the minimal required variables. Preferred `config.toml` locations depends on your runtime and decided with [dirs.rs](https://docs.rs/dirs/latest/dirs/fn.config_local_dir.html):

on linux: `$XDG_CONFIG_HOME/wau/config.toml` or `$HOME/.config/config.toml`, e.g: `/home/gigas/.config/wau/config.toml`
on windows: `{FOLDERID_RoamingAppData}\wau\config.toml`, e.g: `C:\Users\Gigas\AppData\Roaming\wau\config.toml`
on macos: `$HOME/Library/Application Support/wau/config.toml`, e.g: `/Users/Gigas/Library/Application Support/wau/manifest.toml`

additionaly, you can specify config file with `--config` argument

same path strategy is applied to `manifest.toml`

## usage

running `wau` to install or update your addons **requires** first setting required parameters in your `config.toml` and generating the `manifest.toml` file, or copying it to default location

### manifest

`wau manifest` -- generates `manifest.toml` file in default location (see configuration section above)

#### optional args

- `--output %path%` -- the output path for `manifest.toml` file, in case you don't want to override one in your default location
- `--config %path%` -- path to your `config.toml` file in case it's not in default location or you want to use different config file

### update

`wau`/`wau update` -- updates (or installs non-existent) addons using `manifest.toml` data

#### optional args

- `--manifest %path%` -- path to your `manifest.toml` file in case it's not in default location or you want to use different manifest file. This is NOT recommended to do on an existing installation if you don't know what you're doing
- `--config %path%` -- path to your `config.toml` file in case it's not in default location or you want to use different config file

### search

`wau %addon_name%`/`wau search %addon_name%` -- search for addon on supported providers

#### optional args

- `--config %path%` -- path to your `config.toml` file in case it's not in default location or you want to use different config file

### install

`wau install %addon_name%` -- install addon

#### optional args

- `--config %path%` -- path to your `config.toml` file in case it's not in default location or you want to use different config file

### remove

`wau remove %addon_name%` -- removes addon

#### optional args

- `--config %path%` -- path to your `config.toml` file in case it's not in default location or you want to use different config file

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
