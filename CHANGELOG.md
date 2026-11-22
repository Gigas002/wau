# Changelog

## [Unreleased]

### Added

- `[removal] trash = false` in `config.toml`, or `--no-trash` for one run, deletes removed and
  replaced addon folders outright instead of moving them to `<temp dir>/wauT`, which is a copy
  held in RAM when `/tmp` is a tmpfs and fails once it's full. `libwau`: `fs::Removal`,
  `fs::remove`, `GlobalConfig::removal`, `pkg_management::Ctx::removal`, and a `removal`
  argument to `pkg_management::remove`.
- GitHub: `github:owner/repo@<tag-prefix>` installs one release stream of a monorepo that tags
  several addons separately (e.g. an addon and its voice packs). Each prefix is its own package in
  the lock file. Plain `owner/repo` resolution is unchanged.

### Changed

- Dates use `jiff` instead of `chrono`, which is unmaintained (RUSTSEC-2026-0543). Lock files
  read the same; `libwau`'s date fields (`date_published`, `last_updated`, `install_time`,
  `SearchOptions::start_date`) are now `jiff::Timestamp`.

### Fixed

- A corrupt archive broke the addon it was updating: the old folders were already in the trash and
  a half-extracted one took their place. Archives are now extracted to a hidden directory in
  `AddOns` first and moved into place only once complete; installs leave nothing behind either.
- `wau init`: Ctrl-C in the source picker only skipped that group, and without a terminal every
  group was skipped silently. Ctrl-C now stops `init` and a missing terminal is an error; `Esc`
  still skips.
- A `.toc` file starting with a UTF-8 byte order mark lost its first line (usually
  `## Interface`), and one that isn't UTF-8 (Latin-1 author names) hid its folder from `wau init`.
- Unknown sections and keys in `config.toml`, `profile.toml` and `addbuild.toml` were ignored, so
  `[providers.curse]` silently left CurseForge without its key and `flavours = [...]` in an
  addbuild restricted nothing. They're now errors that name the key.
- Text from sources, the catalogue and `.toc` files (names, descriptions, versions, error
  messages) was printed with its control characters, so an escape sequence in an addon's metadata
  reached the terminal. They're now shown as `�`, and line breaks in one-line fields as spaces.
- GitHub: checking a release zip's contents with a ranged read allocated the asset's full size
  (213 MB peak memory for a 200 MB asset), and a server claiming a huge size crashed `wau`. It now
  reads the 25 KB tail in place: 11 MB peak, 20× faster.
- Every install and update left a copy of the downloaded archive in `<cache>/staging`, which
  grew forever. They're now deleted after extraction; `wau cache --clean` removes old ones.
- `wau search`: a huge range in the install prompt (`1-99999999999`) hung, counting through every
  number. Ranges are now clamped to the listed results.
- An archive with no top-level `<Folder>/<Folder>.toc` (a source-code zip, a misnamed git
  `addname`) was reported as installed, with nothing extracted. It's now an error.
- Updating an addon didn't notice that another addon from the same source (or with the same id
  on another source) owned a folder the new version ships. If that folder was missing on disk,
  both ended up owning it; if present, the error called it "unreconciled".
- `wau replace` crashed when the new addon had a dependency, possibly after the old addon had
  already been removed. Dependencies are now installed alongside the replacement.

## [0.1.0] - 2026-10-27

