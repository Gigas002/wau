# Sources

A source is where `wau` resolves and downloads an addon from. Each one is a separate
`libwau::sources::Resolver` implementation behind its own Cargo feature (all enabled by default);
building with `--no-default-features --features <list>` compiles in only the listed ones.

Addons are addressed as `source:alias` (e.g. `wau install curse:big-wigs`), or as a bare URL any
compiled-in source recognises.

---

## Supported sources

In priority order — the order used for tie-breaking when several sources match the same addon
during reconciliation (`wau init`).

| Source        | Id       | Feature        | Alias                                     | URL forms recognised                                                                                       | Credentials                                       |
| ------------- | -------- | -------------- | ----------------------------------------- | ---------------------------------------------------------------------------------------------------------- | ------------------------------------------------- |
| GitHub        | `github` | `github`       | `owner/repo`                              | `https://github.com/<owner>/<repo>[/...]`                                                                  | optional: `[providers.github].api_key` or `handler = "gh"` |
| CurseForge    | `curse`  | `curseforge`   | project slug (or numeric id)              | `https://www.curseforge.com/wow/addons/<slug>`                                                             | required: `[providers.curseforge].api_key`        |
| WoWInterface  | `wowi`   | `wowinterface` | numeric file id                           | `https://www.wowinterface.com/downloads/{info,download}<id>...`, `.../downloads/{landing,fileinfo}.php?...` | none                                              |
| Tukui         | `tukui`  | `tukui`        | addon slug (e.g. `elvui`)                 | —                                                                                                          | none                                              |
| Wago Addons   | `wago`   | `wago`         | project slug/id                           | `https://addons.wago.io/addons/<slug>`                                                                     | required: `[providers.wago].api_key`              |
| Git           | `git`    | `git`          | `addname` (`<config-dir>/addbuilds/<addname>/`) | —                                                                                                    | none (needs `git` + the recipe's `makedepends`)   |

A source missing a required credential stays registered but reports itself disabled ("access token
missing") instead of resolving.

---

## Strategies

Per-addon resolution strategies a source may support. Requesting one a source doesn't declare is a
hard error (`PkgStrategiesUnsupported`), not a silent ignore.

| Source       | `any_flavour` | `any_release_type` | `version_eq` |
| ------------ | :-----------: | :----------------: | :----------: |
| GitHub       |       ✓       |         ✓          |      ✓       |
| CurseForge   |       ✓       |         ✓          |      ✓       |
| WoWInterface |               |                    |              |
| Tukui        |               |                    |              |
| Wago Addons  |               |         ✓          |              |
| Git          |       ✓       |                    |      ✓       |

- `any_flavour` — resolve ignoring the addon's declared flavour compatibility.
- `any_release_type` — include alpha/beta releases, not only stable.
- `version_eq` — pin to an exact version string.

---

## Per-source notes

- **GitHub** — picks a release asset via the packager's `release.json` when present; otherwise by
  `.toc` filename suffix, falling back to the `## Interface:` line inside the archive. Changelogs
  are the release body (Markdown).
- **CurseForge** — files are matched by CurseForge's per-flavour game version type id. Changelogs
  are HTML.
- **WoWInterface** — upstream limitation, preserved as-is: the details API can't express
  classic-specific files for multi-file addons, and the download link always points at the retail
  build regardless of flavour.
- **Tukui** — no strategies; always "whatever the API currently reports," accepted only if one of
  its listed patches maps to the profile's flavour.
- **Wago Addons** — has no mapping for `tbc_classic` or `titan_classic` (see
  [`flavors.md`](flavors.md)).
- **Git** — builds from a local recipe instead of a remote API; see
  [`git-source.md`](git-source.md).
