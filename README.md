# wau

Wow Addon Updater

A lightweight, native World of Warcraft addon manager: a CLI (`wau`) and a reusable library
(`libwau`) that resolve, install, update, and track addons from several sources, backed by a
local TOML lock file per profile.

## runtime dependencies

- [`git`](https://git-scm.com) — required. `wau search` and `wau init` resolve the addon
  catalogue through a local git mirror of `instawow-data` (see `docs/caching.md`); every other
  command works without it.
- [`gh`](https://cli.github.com) — optional. Set `[providers.github].handler = "gh"` to route
  GitHub API requests through the `gh` CLI (authenticated via `gh auth login`) instead of a
  client-side `[providers.github].api_key`. Not needed with the default `handler`.

## usage

See `docs/`:

- `docs/cli.md` — full command/flag surface.
- `docs/caching.md` — what's cached, where, and its caveats.
- `docs/search.md` — the search/ranking algorithm and the interactive select-to-install flow.

## thanks to

- [ajour](https://github.com/ajour/ajour), some implementations are inherited from there
- [instawow](https://github.com/layday/instawow)
- [wowup](https://github.com/WowUp)
