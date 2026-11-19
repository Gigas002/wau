# Audit

A whole-application audit of wau and libwau (2026-11-19) in five parts: bugs
(§2), performance (§3), security and robustness (§4), usability (§5), and
code quality, tests and project hygiene (§6). §7 lists what was left open,
and §8 is the checklist to re-run the audit.

Each finding says what was found, how serious it is, and what was done:
**fixed** (with the commit), **accepted** (with the reason), or **OK**
(checked, nothing to do). Every fix is its own commit, with a regression test
where one can be written.

Severity: **high** (a crash in normal use, or an addon or the lock file left
broken), **medium** (wrong behavior in a reachable case, an ignored
setting), **low** (hardening, polish, or needs an unusual setup).

## 1. Method

Every module of both crates was read; the progress bus and the error types
were only skimmed. Findings were then reproduced in three ways:

- **Probe tests** in the crates' own harnesses: package operations against
  fake `AddOns` directories, archives built for the case (empty, nested,
  corrupted after the CRC was written), `.toc` files with a BOM or in
  Latin-1, and every source through mockito. No test touches a real WoW
  install, `~/.config/wau`, `~/.cache/wau` or a live API.
- **Measurements** on release builds of the base (`3311f53`) and the final
  tree, with peak RSS from `wait4`: a GitHub ranged read of a 200 MB asset,
  a 60k-entry catalogue, `wau list` on 500 packages.
- **The release binary** against copies of the `testing/` environments, with
  `HOME` and the XDG dirs pointed at a scratch directory.

Builds checked: default, `--no-default-features`, `--all-features`. Static
checks: `cargo clippy` at the three gate levels, `cargo deny check`, `typos`,
`cargo llvm-cov`.

## 2. Bugs

| Area | Finding | Status |
| ---- | ------- | ------ |
| Replace with dependencies | `replace` looked up the old package for every downloaded candidate with `expect("every candidate has an inverse old defn")`. A dependency of the new package has none, so `wau replace curse:old curse:new` panicked whenever the new addon declared one, possibly after the old one had been removed and the lock saved. Dependencies are now installed alongside. | Fixed, high (5a51758) |
| Folder conflicts on update | The check that no other package owns a folder skipped `source != s && id != i`, so it also skipped every other package on the same source and every same-id package on another. Updating `curse:1` to a version shipping a folder `curse:2` owns left both owning it, and removing either later trashed the other's files. A test had pinned the quirk. | Fixed, medium (151afca) |
| Corrupt archives | `update` trashed the old folders, then extracted. A CRC error left the corrupt bytes in place of the working addon, with the lock naming the old version; a failed install left partial folders. Archives are extracted into a hidden directory in `AddOns` first and renamed into place once complete. | Fixed, medium (f3894dd) |
| Archives with no addon folder | An archive with no top-level `<Folder>/<Folder>.toc` (a source-code zip, a misnamed git `addname`) installed "successfully" with nothing extracted and no folders recorded. It's now `PkgFilesMissing`. | Fixed, medium (7b5184e) |
| `.toc` encodings | A UTF-8 BOM hid the first line, usually `## Interface`. A `.toc` that isn't UTF-8 (`## Author: Müller` in Latin-1) failed `read_to_string`, and the folder disappeared from `wau init`. | Fixed, medium (7716aa8) |
| Search prompt ranges | `1-100000000000` in `wau search`'s install prompt counted through every number (24.8 s); a 20-digit bound would never finish. Ranges are clamped to the list. | Fixed, low (00eb00b) |
| Lock file saves | Written to a temporary file and renamed, so an interrupted save leaves the old file. | OK |

## 3. Performance

| Area | Finding | Status |
| ---- | ------- | ------ |
| GitHub ranged reads | To list a release zip without downloading it, the 25 KB tail was copied into a zeroed buffer of the asset's full size, then cloned. For a 200 MB asset: **212.7 MB → 10.9 MB** peak RSS and **15.1 → 0.75 ms** per resolve (3 runs each, mockito serving the real tail). A tail that covers the whole file now counts as complete, saving a second download. | Fixed, medium (b78ea38) |
| Download staging | Every install, update and replace wrote the archive to `<cache>/staging` and never deleted it, on top of the HTTP cache's copy: one extra archive per operation, forever. Now **0** files remain after an install and an update (test). | Fixed, medium (39353bc) |
| chrono → jiff | Neutral. Catalogue parse of a synthetic 60k-entry catalogue, best of 15: **72.0 ms** (chrono) vs **73.3 ms** (jiff). Release binary **14.82 → 15.23 MB** (+409 KB); `Cargo.lock` 285 → 278 packages. | OK (1e2c139) |
| `wau list` | 500 packages and a 2000-entry version log, median of 40 runs, base vs final: simple **17.6 / 17.6 ms**, detailed **17.5 / 17.9 ms**, json **18.6 / 18.7 ms**; peak RSS **16.1 / 16.4 MB**. | OK |
| Trash | `fs::trash` moves removed folders to `<temp dir>/wauT`. `/tmp` is usually tmpfs on a different filesystem from `AddOns`, so each removal is a copy: **~300 ms and 196 MB of RAM**, held until reboot, for a 196 MB addon (a same-filesystem rename takes 0.07 ms). Once `/tmp` is full, trashing fails, and so do update and remove. | Accepted, see §7 |
| HTTP cache | Archives are cached `Indefinite` and never evicted. `wau cache` shows the size and `wau cache --clean` removes it. | Accepted, see §7 |
| Extraction | One pass, entries streamed to disk; the extract directory adds a rename per folder. | OK |

Not measured: anything against live sources (resolve, download and catalogue
sync were exercised only through mockito and local files, so as not to hit
the real APIs), search ranking on the real catalogue (a synthetic one was
used), and extraction throughput.

## 4. Security and robustness

| Area | Finding | Status |
| ---- | ------- | ------ |
| Terminal escapes | Names, slugs, descriptions, versions and URLs from sources, the catalogue and `.toc` files, and error text quoting servers, were printed as-is. An escape sequence in an addon's metadata could set the terminal title, write the clipboard (OSC 52) or hide text in `list`, `search` and `init`. Control characters now print as `�`. JSON was already escaped. | Fixed, medium (3884ad1) |
| Server-sized buffers | A `206` whose `Content-Range` claimed 1 TiB aborted the process ("memory allocation of 1099511627776 bytes failed"). Ranged tails are now read in place; a test resolves exactly that response. | Fixed, medium (b78ea38) |
| Dependencies | `cargo deny check` failed at the base: chrono is unmaintained (RUSTSEC-2026-0543), which ARCHITECTURE.md §7.2 rules out. Dates are `jiff::Timestamp` now (UTC only, no tz database); lock files read the same, and a test loads one written by chrono. | Fixed, medium (1e2c139) |
| Archive paths | Extraction uses `enclosed_name` (entries with `..` or absolute paths are skipped) and keeps only top-level folders whose name passes `is_safe_folder_name`. A symlink entry is written as a plain file holding its target, so nothing is created outside `AddOns`. | OK |
| Removing folders | Every trash goes through `is_within(addon_dir, ..)`, so a lock entry naming `..` can't remove anything outside `AddOns` (existing test). | OK |
| Shared trash dir | `/tmp/wauT` is a fixed name in a world-writable directory. Another user can create it first, so wau's trashing fails, or files land in a directory they can read (addon files, a profile's `profile.toml`/`lock.toml`; no secrets). | Accepted, low, see §7 |
| Checksums | Downloads aren't verified, although CurseForge publishes file hashes. Documented in `docs/caching.md`. | Accepted, see §7 |
| Tokens | `SecretString` redacts `Debug`/`Display`. reqwest drops `Authorization` on a cross-host redirect. With `handler = "gh"`, downloads go out unauthenticated (documented). | OK |
| Git sources | Build scripts run from the user's own `addbuilds` dir; `addname` is checked with `is_safe_folder_name`; `zip_dir` skips symlinks. Trusted by design. | OK |
| Concurrent runs | Lock saves are atomic, but two `wau` processes can each load, change and save the lock, and the second save drops the first's change. | Accepted, see §7 |

## 5. Usability

| Area | Finding | Status |
| ---- | ------- | ------ |
| Unknown config keys | A typo in `config.toml`, `profile.toml` or `addbuild.toml` was ignored. `[providers.curse]` (the source id; the section is `curseforge`) left CurseForge with "access token missing"; `flavours = [...]` in an addbuild (the key is `flavors`) restricted nothing. These are now errors that name the key. | Fixed, medium (52ec6f5) |
| `init` prompts | Ctrl-C in the source picker only skipped that group (and then the next profile), and without a terminal every group was skipped and `init` succeeded. Ctrl-C now stops `init` and a missing terminal is an error; `Esc` still skips. | Fixed, low (f2951a8) |
| Docs | `docs/caching.md` said there's no cache clear command (`wau cache --clean` exists) and that staging grows without bound; `docs/git-source.md` said a wrong `addname` "silently installs zero folders"; `docs/cli.md` didn't say what `Esc` and Ctrl-C do in `init`. All updated with their fixes. | Fixed, docs (39353bc, 7b5184e, f2951a8) |

## 6. Code quality, tests and hygiene

| Area | Finding | Status |
| ---- | ------- | ------ |
| Tests writing to `/tmp` | The `fs`, `config` and `pkg_management` tests trashed real files into the shared `/tmp/wauT`: 37 directories after three gate runs. Under `cfg(test)` the trash root is per test, and a test that forgets to set it panics. | Fixed (293f920) |
| Coverage | 80.66 % → 81.56 % of lines, 75.56 % → 76.75 % of regions (`cargo llvm-cov --workspace --all-features`). The CLI suite (`wau/tests/cli.rs`, `#[ignore]`) isn't counted. | Fixed |
| New tests | Every test binary passed 20 parallel runs after each commit; no test waits on time. | OK |
| Cargo warnings | `package.homepage` repeated `package.repository` and warned on every build. | Fixed (f94b612) |
| Gates | fmt, typos, clippy and tests at the three feature levels, the ignored CLI suite and docs pass on every commit. `cargo deny check` failed from the base until 1e2c139 (chrono) and passes since. | OK |

## 7. Left open

- **Where the trash lives.** `<temp dir>/wauT` costs a copy and RAM on tmpfs
  (§3), fails updates once `/tmp` is full, and is shared between users (§4).
  Trashing into the cache dir (same filesystem as most installs, shown and
  cleaned by `wau cache`), the freedesktop trash, or deleting outright are
  product decisions, and `fs::trash` would need the cache dir passed in.
- **Download checksums.** CurseForge publishes hashes; checking them would
  catch a corrupt cache entry or download before extraction.
- **HTTP cache eviction.** `Indefinite` entries (archives, changelogs) stay
  until `wau cache --clean`. An age or size limit would bound it.
- **Concurrent runs.** An advisory lock on the profile's `lock.toml` would
  stop two `wau` processes losing each other's changes.
- **Archive size limit.** Extraction has no limit on total size or entry
  count. Archives come from the sources the user chose, so this is low.
- **libwau API.** The jiff move changed the type of the public date fields;
  libwau is pre-1.0, so it is only noted in the changelog.

## 8. Re-running the audit

```sh
# Gates (docs/ARCHITECTURE.md §8)
cargo fmt --all -- --check && typos && cargo deny check
for f in --no-default-features "" --all-features; do
  cargo clippy --workspace --all-targets $f -- -D warnings
  cargo test --workspace $f
done
cargo doc --workspace --no-deps

# CLI suite (spawns the binary on copies of testing/; keep HOME away from ~/.config)
HOME=$(mktemp -d) cargo test -p wau --test cli -- --ignored

# Coverage (system LLVM tools on Arch: LLVM_COV / LLVM_PROFDATA)
LLVM_COV=/usr/bin/llvm-cov LLVM_PROFDATA=/usr/bin/llvm-profdata \
  cargo llvm-cov --workspace --all-features --summary-only
```

To compare `wau list` timings, point `--config`/`--profile` at a copy of
`testing/retail` with a generated `lock.toml`, and time the release binary
with `HOME` and the XDG dirs set to a scratch directory.
