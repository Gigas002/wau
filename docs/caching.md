# Caching

`wau` caches three independent things, each with its own model — there's no single unified
cache, and no single place that "turns caching on." Understanding which one applies to a given
request matters for interpreting stale results.

1. HTTP responses (`libwau::http`) — API lookups, changelogs, and (via the same mechanism)
   downloaded addon archives.
2. The addon catalogue (`libwau::catalogue::git_cache`) — a local mirror of a git branch.
3. Downloaded archives — not a cache of its own; piggybacks on (1), see below.

All three live under one profile-shared directory: `Dirs.cache` (`[paths].cache` in
`config.toml`, default `~/.cache/wau` via the `dirs` crate). This directory is part of
`GlobalConfig`, not `ProfileConfig` — every profile read from the same `config.toml` shares one
cache dir (and, since `access_tokens` are global too, shares the same credentials for whatever's
cached under a given key).

---

## HTTP response cache

`libwau::http::HttpClient` wraps a flat-file cache (`libwau/src/http/cache.rs`) under
`<cache_dir>/http/`: one `<hash>.meta` (JSON: status, headers, expiry) and one `<hash>.body` (raw
bytes) per entry, keyed by a hash (`std::hash::DefaultHasher`, **not** cryptographic — fine for
a cache key, not a security boundary) of `METHOD URL\nheader:value\n...` (headers sorted, so
order doesn't affect the key).

Caching only happens when the client is built via `HttpClient::with_cache_dir(&Path)` — the
only real call site is `wau::ctx::AppCtx::from_profile`. Every test constructs `HttpClient::new()`
instead (no cache dir at all), so the ~50 existing test call sites never needed touching when
this was added. Only responses with status `200`, `206` (GitHub's ranged reads), or `501`
(GitHub mislabels an out-of-range request as this) are ever stored.

Every call site picks its own policy explicitly via `CacheTtl`:

- `Never` — bypass the cache entirely.
- `For(Duration)` — cache for a fixed window.
- `Indefinite` — cache forever once fetched; relies on the URL being version-specific/immutable.

Current per-call-site TTLs:

| Source / call | TTL |
| --- | --- |
| Tukui addon lookup | 5 min |
| Wago addon lookup | 5 min |
| CurseForge mod lookup (numeric id or slug search) | 15 min |
| CurseForge files list | 1 h |
| CurseForge `mod_ids` batch POST | 5 min |
| WoWInterface batch details | 15 min |
| GitHub repo lookup | 1 h |
| GitHub releases list | 5 min |
| GitHub `release.json` (packager metadata) | 24 h |
| Changelogs (every source, `default_get_changelog`) | Indefinite |
| GitHub asset zip download (ranged tail + full-download fallbacks) | Indefinite |
| Package archive download (`pkg_archives::download_pkg_archive`) | Indefinite |

The short TTLs (5–15 min) are on the calls that answer "does a new version exist" — short enough
that `wau sync`/`wau search` notice new releases promptly, long enough that repeatedly invoking
`wau` doesn't hammer a source's API. `Indefinite` is only used where the URL itself is
version-specific (a changelog for one exact release, a zip for one exact tagged asset) — the
comment in `libwau/src/http/mod.rs` on `CacheTtl::Indefinite` says so directly.

### Caveats

- **The `gh` CLI handler bypasses this cache entirely.** `[providers.github].handler = "gh"`
  routes GitHub API calls through `gh api` (`sources::github::gh_cli`) instead of `reqwest`; the
  real (non-test) code path discards whatever `CacheTtl` was requested (`let _ = (http, ttl);` in
  `gh_cli::get`) and always shells out. Every GitHub metadata lookup under `handler = "gh"` hits
  the network (or `gh`'s own state) on every call, with no TTL/indefinite caching at all.
- **The final asset byte-download never goes through `gh`, even under `handler = "gh"`.**
  `pkg_management::download_all` always downloads via the shared `ctx.http` (the plain
  `reqwest`-backed client), never via `GitHubResolver::fetch`/`gh_cli`. `make_request_headers`
  only attaches an `Authorization` header for `handler = "Token"` — under `handler = "gh"` the
  download request goes out unauthenticated. This works for public repos; a private repo's
  release asset will likely 403 even though `gh auth login` successfully resolved it, since
  resolving and downloading are two different request paths with different auth.
- **No conditional revalidation.** There's no `ETag`/`If-None-Match`/`Last-Modified` support —
  purely TTL-based. If upstream data changes inside a `For(...)` window, or an `Indefinite` URL's
  assumption ("this content never changes") turns out false, stale bytes are served until the
  TTL expires (never, for `Indefinite`) with no way to detect or force a refresh short of deleting
  the cache file.
- **No `wau cache clear` / `--no-cache` flag.** Both existed at one point and were removed along
  with the rest of the CLI surface trimming (see `docs/WAU_RS_PLAN.md`'s revision history);
  restoring the cache mechanism didn't bring them back. Clearing the cache today is `rm -r
  <cache_dir>/http` by hand.
- **Shared across every profile under one `config.toml`.** Since `Dirs.cache` is per-`GlobalConfig`
  and not per-profile, two profiles pointed at the same config share cached responses — not a
  correctness problem (the cache key includes headers, so different tokens/URLs never collide),
  just something to know if you're inspecting the cache dir and expecting a per-profile split.

---

## Catalogue mirror

`libwau::catalogue::synchronise` no longer fetches the published catalogue JSON with a plain
`GET`. Instead `catalogue::git_cache::resolve` maintains a shallow, sparse `git` clone of
[`layday/instawow-data`](https://github.com/layday/instawow-data)'s `data` branch under
`<cache_dir>/catalogue-data`:

- **First run**: `git clone --depth=1 --filter=blob:none --no-checkout --branch data <url> <dir>`,
  then `sparse-checkout init --no-cone` + `sparse-checkout set /<filename>` + `checkout data` —
  only the one pinned file (`base-catalogue-v{CATALOGUE_VERSION}.compact.json`) is ever
  materialised, not the whole branch (~4.5 MB vs. ~66 MB for every version instawow-data has ever
  published).
- **Later runs**: `sparse-checkout set` (re-widens the checkout if the pinned filename changed
  since last run, e.g. after a `wau` upgrade), `fetch --depth=1 origin data`, `reset --hard
  FETCH_HEAD`.
- A failed update (network down, `git` missing, remote unreachable) falls back to the existing
  on-disk copy with a `tracing::warn!` instead of hard-failing — a stale catalogue beats no
  catalogue. Only a **first-ever** clone failure (nothing cached yet) propagates as an error.

`CATALOGUE_VERSION` (currently `8`) is a manually-bumped constant, not resolved to "whatever's
newest in the branch." The catalogue JSON's shape (`RawCatalogueEntry`/`RawCatalogue`) is
versioned to match a specific `CATALOGUE_VERSION`; auto-resolving to the latest available file
would risk silently feeding this code a schema it wasn't written to parse.

### Caveats

- **Unlike the HTTP cache, this is not TTL-gated at all.** Every call to `synchronise` (once per
  `wau search` invocation, once per `wau init` run — shared and lazy across a multi-profile
  `init` loop, so an all-clean profile set never touches it) attempts a live `git fetch`, not
  "was this checked in the last N minutes." The round trip is small (one shallow fetch against a
  branch tip), but it is a network operation on every relevant invocation, every time.
- **Requires a system `git` binary on `PATH`.** There's no fallback to plain HTTP if `git` isn't
  installed — `run_git`'s `tokio::process::Command::new("git")` spawn failure surfaces as an
  `InternalError`, and (per the point above) only gets swallowed into a stale-copy fallback if a
  previous successful clone already exists.
- **`CATALOGUE_VERSION` is a standing maintenance burden.** If `instawow-data` ever stops
  publishing `base-catalogue-v8.*`, `synchronise` starts failing (`{filename} not found on ...`)
  until `wau` is updated to a `CATALOGUE_VERSION` upstream still publishes — there's no
  automatic detection of "the pin is stale," just a resolve failure at the moment it happens.

---

## Downloaded archives

Not a separate cache — `pkg_archives::download_pkg_archive` fetches through the same
`HttpClient` as everything else in this document, with `CacheTtl::Indefinite`. A specific
version's `download_url` is assumed immutable, so once fetched it's served from
`<cache_dir>/http/` on every later `install`/`update` — by any profile sharing that cache dir —
without another network round trip.

### Caveats

- **Relies entirely on resolvers returning version-specific URLs.** If a source ever handed back
  a mutable "latest" URL instead of one scoped to an exact release/file, this cache would serve
  the first-ever-fetched bytes forever regardless of what that URL now points to. Every current
  resolver returns per-version URLs, so this holds today, but it's an assumption, not something
  enforced in code.
- **A cache hit still writes a fresh file to `<cache_dir>/staging/`.** The HTTP-level cache avoids
  the *network* fetch, but `download_pkg_archive` always writes the response body out to
  `<cache_dir>/staging/download-<pid>-<nanos>` before returning — a new, uniquely-named file every
  single call, cache hit or not. Nothing in `pkg_management` ever deletes these after extraction.
  `<cache_dir>/staging` grows without bound over time; periodic manual cleanup (`rm -r
  <cache_dir>/staging`) is needed until this gets its own eviction.
- **No checksum verification anywhere in this path** (documented directly on
  `libwau/src/pkg_archives/download.rs`) — despite CurseForge exposing file hashes in its API,
  they're never checked against the downloaded bytes, cached or not.
