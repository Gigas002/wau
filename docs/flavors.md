# Flavors

A flavor (`libwau::model::Flavour`) is the WoW client track a profile targets. It decides which
release files a source picks and which `.toc` files count as loadable.

Each profile either auto-detects its flavor from the install subfolder in its `path` (e.g.
`.../_retail_/Interface/AddOns`), or forces one with `flavour = "<id>"` in `profile.toml`. If
neither works, the profile fails to load with "game flavour cannot be detected".

---

## Supported flavors

| Flavor               | Id                | Aliases accepted | Client versions                      | `.toc` suffixes (priority order) |
| -------------------- | ----------------- | ---------------- | ------------------------------------ | -------------------------------- |
| Retail               | `mainline`        | `retail`         | original `1.x`–`5.x` builds, `6.0`–`13.0` | `Mainline` |
| WoW: Forever         | `forever`         | `camelot`        | `1.60`–`1.70`                        | `Forever`, `Camelot`, `Mainline` |
| Classic Era          | `vanilla_classic` |                  | `1.13`–`2.0` (excluding Forever)     | `Vanilla`, `Classic`             |
| TBC Classic          | `tbc_classic`     |                  | `2.5`–`3.0`                          | `TBC`, `BCC`, `Classic`          |
| Wrath Classic        | `wrath_classic`   |                  | `3.4`–`3.8`                          | `Wrath`, `WOTLKC`, `Classic`     |
| Titan Classic        | `titan_classic`   |                  | `3.8`–`4.0`                          | `Wrath`, `WOTLKC`, `Classic`     |
| Cataclysm Classic    | `cata_classic`    |                  | `4.4`–`5.0`                          | `Cata`, `Classic`                |
| Mists Classic        | `mists_classic`   | `classic`        | `5.5`–`6.0`                          | `Mists`, `Classic`               |

Ranges are half-open (`start`–`end` excludes `end`). `classic` is a *moving alias* for whichever
classic-progression flavor is current (Mists of Pandaria as of this writing), not a distinct client.

---

## Auto-detection

The install subfolder name maps to a flavor:

| Subfolder                                                                              | Flavor            |
| -------------------------------------------------------------------------------------- | ----------------- |
| `_retail_`, `_beta_`, `_ptr_`, `_alpha_`, `_event1_`, `_dark_realm_`, `_dark_realm_2_` | `mainline`        |
| `_forever_`                                                                            | `forever`         |
| `_classic_era_`, `_classic_alpha_`                                                     | `vanilla_classic` |
| `_anniversary_`, `_classic_era_ptr_`                                                   | `tbc_classic`     |
| `_classic_`, `_classic_beta_`, `_classic_ptr_`                                         | `mists_classic`   |

Vendor builds (`_vendor_`, `_vendor2_`, …) are also mapped; see `PRODUCTS` in
`libwau/src/model/mod.rs` for the full table. Any other folder name (e.g. a private server's
custom directory) needs `flavour` set explicitly.

---

## Per-source support

| Flavor            | GitHub | CurseForge | WoWInterface | Tukui | Wago Addons | Git |
| ----------------- | :----: | :--------: | :----------: | :---: | :---------: | :-: |
| `mainline`        |   ✓    |     ✓      |      ✓       |   ✓   |      ✓      |  ✓  |
| `forever`         |   ✓    |     ✓      |      ✓¹      |   ✓   |      ✓      |  ✓  |
| `vanilla_classic` |   ✓    |     ✓      |      ✓¹      |   ✓   |      ✓      |  ✓  |
| `tbc_classic`     |   ✓    |     ✓      |      ✓¹      |   ✓   |             |  ✓  |
| `wrath_classic`   |   ✓    |     ✓      |      ✓¹      |   ✓   |      ✓      |  ✓  |
| `titan_classic`   |   ✓    |     ✓      |      ✓¹      |   ✓   |             |  ✓  |
| `cata_classic`    |   ✓    |     ✓      |      ✓¹      |   ✓   |      ✓      |  ✓  |
| `mists_classic`   |   ✓    |     ✓      |      ✓¹      |   ✓   |      ✓      |  ✓  |

¹ WoWInterface always serves the retail build's download link regardless of flavor (upstream
limitation, see [`sources.md`](sources.md)).

Tukui and Git are only as good as the addon itself: Tukui accepts an addon only if one of its
listed patches falls in the flavor's version range, and Git only if the recipe's `addbuild.toml`
`flavors` is unset or lists that flavor (see [`git-source.md`](git-source.md)).
