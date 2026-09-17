# The `git` source

`git` is for addons that only exist as a git repository with no packaged release: modeled directly on how `paru`/`yay` build AUR `-git` packages — a small, user-authored recipe (their PKGBUILD; here, `addbuild.toml` + a build script) describing where the source lives and how to turn a checkout into an installable addon folder, with the version derived from git history rather than a field anyone maintains by hand.

---

## Quick start

1. Create `<config-dir>/addbuilds/<addname>/` (`<config-dir>` is the same platform-conventional
   directory `config.toml`/`profiles/` live in — `~/.config/wau` on Linux by default).
2. Write `addbuild.toml` (spec below) and a build script inside it.
3. `chmod +x` the build script.
4. `wau install git:<addname>`.

`examples/addbuilds/{TomTom,Details,ZSBT}/` are worked examples (real repos, actually installable)
— `ZSBT/addbuild.toml` is the most heavily annotated and worth reading first. Note that each
directory/`addname` matches the addon's real in-game folder name exactly, **not** necessarily the
upstream repo's own directory name (see `Details/addbuild.toml`, whose repo is named
`Details-Damage-Meter` but whose addon folder — and every `.toc` in it — is `Details`): the match
against `<Folder>/<Folder>.toc` inside the built zip (same rule every other source's zip goes
through, see below) is case-sensitive, so getting this wrong silently installs zero folders
instead of failing loudly.

---

## `addbuild.toml` spec

| Field         | Type             | Required                                            | Meaning                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ------------- | ---------------- | --------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `addname`     | string           | yes                                                 | Must equal the containing directory name (`<addbuilds_dir>/<addname>/`). Validated on every resolve — a mismatch is a hard error, not a warning.                                                                                                                                                                                                                                                                                |
| `url`         | string           | yes                                                 | Git remote passed straight to `git clone`/`git fetch`. Anything `git` itself accepts (`https://`, `ssh://`, a local path, ...).                                                                                                                                                                                                                                                                                                 |
| `makedepends` | array of strings | no (default `[]`)                                   | Executable names checked for on `PATH` (`sh -c 'command -v ...'`) before anything else runs. Missing ones are a hard error listing all of them — **wau never installs these itself**, unlike `paru`'s auto-install-via-`pacman` for AUR makedepends. `git` itself isn't implicitly required here; list it anyway if you want the preflight to catch its absence with a clear message rather than an opaque `git` spawn failure. |
| `flavors`     | array of strings | no (default: unset = compatible with every flavour) | WoW client flavours this addon supports, parsed with the same names `[profile].flavour` uses (`mainline`, `vanilla_classic`, `tbc_classic`, `wrath_classic`, `titan_classic`, `cata_classic`, `mists_classic`, plus the `classic`/`retail` aliases). An unparsable entry is a hard error (fail on a typo, don't silently ignore it).                                                                                           |
| `depends`     | array of strings | no (default `[]`)                                   | Other addons this one needs installed alongside it, each a full `source:alias` addon URI — the exact same format `wau install`/`wau remove` take on the command line (e.g. `wowi:1234`, `curse:details`). Validated eagerly (must parse as a `Defn` URI) before any clone/build; an unparsable entry is a hard error. See [Dependencies](#dependencies-depends).                                                               |
| `build`       | string           | yes                                                 | Path to the build script, relative to `addbuild.toml`'s own directory.                                                                                                                                                                                                                                                                                                                                                          |

```toml
# ~/.config/wau/addbuilds/ZSBT/addbuild.toml
addname = "ZSBT"
url = "https://github.com/Zorellion/ZSBT"
makedepends = [ "git", "tar" ]
flavors = [ "mainline" ]
# depends = [ "wowi:1234" ]
build = "build.sh"
```

---

## Installing and updating

`wau install git:<addname>` and `wau sync` both go through the ordinary `pkg_management`
resolve/install pipeline — nothing about `git` is special-cased there. What _is_ different is
where the work happens: every other source's resolver calls a remote API and hands back a
`download_url` to fetch; `GitResolver::resolve_one_impl` does the **entire** clone, version
computation, build, and packaging itself, then hands back a `file://` URI pointing at a zip it
just built. `pkg_management`/`pkg_archives` treat that exactly like any other zip download —
they don't know or care it was produced locally.

Per-resolve steps, in order (cheapest checks first):

1. **Load `addbuild.toml`** from `<addbuilds_dir>/<addname>/`. Missing directory/file → "package
   does not exist" (same class of error as a 404 from a real API); a parse error or
   `addname` mismatch → a distinct "this local file is broken" error.
2. **Flavour gate** — if `flavors` is set and the profile's flavour isn't in it (and
   `#any_flavour` wasn't requested), fail before touching the network at all.
3. **`makedepends` preflight** — every listed executable must resolve on `PATH`.
4. **Clone or update** `<cache_dir>/git-src/<addname>`: a full clone (never `--depth`-shallow) if
   it doesn't exist yet or its `origin` remote no longer matches `addbuild.toml`'s `url` (editing
   `url` after a checkout already exists forces a fresh clone — a plain `fetch` would otherwise
   silently keep pulling from the old remote forever, since it never repoints `origin`), else
   `git fetch` + hard-reset to the remote's default branch tip + `git clean -fdx`. Full history is
   required because the version scheme below needs `git rev-list --count` to keep increasing
   across runs — a shallow clone would break that.
5. **Determine version**: `r<rev-count>.<short-hash>` of whatever commit is now checked out
   (`HEAD`, or a pinned commit — see [Pinning](#pinning-strategyversioneq)). This is _always_
   computed this way; nothing in `addbuild.toml` or the build script can override it.
6. **Build, with a cache**: if `<cache_dir>/git-build/<addname>/<version>.zip` already exists,
   reuse it — the build script does not run again for a version already built. Otherwise, run the
   build script (see next section) and zip its output to that path.
7. Return a candidate: `id`/`slug`/`name` = `addname`, `url` = the addbuild's `url`,
   `download_url` = `file://<the zip>`, `version` as computed, `date_published` = the checked-out
   commit's date, `changelog_url` = a `data:` URI holding the last 10 `git log --oneline` lines.

Because step 4 always does a real `git fetch`, every `wau sync` (or any resolve at all) is a
network operation for every installed `git` package — same as `paru -Syu` re-checking every
`-git` package's remote on each run. Step 6's cache is what keeps that cheap: the fetch happens
every time, the (potentially expensive) build only reruns when the version actually changed.

---

## The build script contract

The build script is run **directly** — `wau` does not choose an interpreter for you. Whatever
shebang line the file starts with decides that (`#!/bin/sh`, `#!/usr/bin/env bash`,
`#!/usr/bin/env nu`, ...), so **the script must be executable** (`chmod +x`). A non-executable
script fails with a clear message telling you to `chmod +x` it, not an opaque OS spawn error.

It runs with `cwd` set to the git checkout, and these environment variables (PKGBUILD's own
lowercase naming):

| Var        | Value                                                                                                                                                             |
| ---------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `srcdir`   | The git checkout (same as `cwd`).                                                                                                                                 |
| `pkgdir`   | An empty directory the script must populate with the final top-level addon folder(s). Whatever's here when the script exits becomes the installed zip's contents. |
| `pkgname`  | The addname.                                                                                                                                                      |
| `pkgver`   | The computed `r<rev-count>.<short-hash>` version string.                                                                                                          |
| `startdir` | The addbuild's own directory (where `addbuild.toml` and the script itself live) — for sibling files such as patches.                                              |

A non-zero exit is a hard error (its stderr is captured and surfaced). There is no `pkgver()`
step to implement — unlike a real PKGBUILD, versioning is entirely wau's job (see step 5 above);
the script's only job is producing the right files under `pkgdir`.

`pkgdir`'s contents are zipped up and handed to the same top-level-folder detection every other
source's zip goes through (`<Folder>/<Folder>.toc`, case-insensitive, exactly one level deep) —
anything else the script leaves in `pkgdir` is silently ignored rather than installed. This means
a misbehaving or overly generous build script can't smuggle in unexpected folders; it's free
defense-in-depth, not something the build script needs to get right on its own.

A no-op build for addons whose repo root already looks like the in-game folder (the common case
for pure-Lua addons — most `-git` packages need no real build step):

```sh
#!/bin/sh
set -e
mkdir -p "$pkgdir/$pkgname"
git -C "$srcdir" archive HEAD | tar -x -C "$pkgdir/$pkgname"
```

`git archive` (not a plain `cp -r`) exports exactly what's tracked at the checked-out commit —
no `.git` directory, no untracked/`.gitignore`d build artifacts riding along into the installed
addon folder. (This is exactly what `examples/addbuilds/{TomTom,ZSBT}/build.sh` do — real,
working addons, not just illustrative stubs.) A real build step — a Makefile, a Lua minifier,
vendoring a dependency, **splitting one repo into several addon folders** — just needs to leave
the right folder(s) under `$pkgdir` by the time it exits; how it gets there is entirely up to the
script.

`examples/addbuilds/Details/build.sh` is the non-trivial case: Details' repo bundles several
plugins under `plugins/<name>/`, each really its own independent addon (its own top-level
`.toc`) that the real CurseForge distribution installs as a sibling folder, not nested inside
`Details/` — a plain single-folder copy gets this wrong (see the caveat below). Its build script
exports the tree once, copies everything _except_ `plugins/` into `$pkgdir/$pkgname/`, and copies
each `plugins/*` subdirectory out to its own top-level `$pkgdir/<plugin-name>/` alongside it —
`$pkgdir` ending up with more than one top-level folder is completely normal; nothing about the
resolve pipeline assumes a git addon is single-folder.

---

## Strategies

`git` declares support for two of the three strategies `Defn` can request
(`git:<addname>#strategy1,strategy2=value`):

### `#any_flavour`

Bypasses the `flavors` allow-list check (step 2 above). No effect if `addbuild.toml` doesn't set
`flavors` in the first place, since there's nothing to bypass.

### `#version_eq=<version>` (pinning `Strategy::VersionEq`)

```
wau install "git:ZSBT#version_eq=r42.abc1234"
```

Parses the commit hash back out of a previously-generated `r<count>.<hash>` string and does
`git checkout --detach <hash>` in the cached checkout before computing the version (which will
then round-trip back to the same string, since it's recomputed from whatever's checked out).
Only accepts wau's own generated format — an arbitrary tag/branch/full-length hash is not
accepted, and neither is an unknown/nonexistent hash; both fail as "no files found for the
requested strategies," same as a version-pin miss on any other source.

`Strategy::AnyReleaseType` is **not** supported — there's no concept of a release channel for a
raw git checkout, so requesting `#any_release_type` fails the same generic "strategy not valid
for source" check every other source applies.

---

## Dependencies (`depends`)

```toml
depends = [ "wowi:1234", "curse:details" ]
```

Each entry is a full addon URI in whatever format `wau install`/`wau remove` already accept —
`source:alias`, optionally with a `#strategy` fragment (e.g. `curse:details#any_release_type`).
`GitResolver` validates every entry parses as a `Defn` (via the same `Defn::from_uri` the CLI
itself uses) _before_ touching the network at all — a typo is a hard error at resolve time, not
a confusing failure three steps later.

Validated entries are carried straight through as `PkgCandidate::deps`. From there they go
through the exact same one-level dependency resolution every other source already has
(`pkg_management::resolve_deps` — CurseForge mods depending on other CurseForge mods is the
instawow-parity case this was originally built for): `wau install git:<addname>` resolves and
installs each dependency alongside the git package itself, in the same batch. The only thing
`git` adds is that its dependency ids are **cross-source** (a raw `source:alias` string) rather
than a bare same-source id — `pkg_management::parse_dep_defn` tells the two apart by whether the
id contains a `:` (no source's own native dependency ids ever do).

Dependencies are resolved independently per addon: a dependency that fails to resolve is
reported as its own error and does not block the git package's own installation, matching how a
CurseForge dependency failure behaves today. There is no dependency-uninstall-cascade either —
removing the git package later does not remove what `depends` pulled in.
