# Changelog

## [Unreleased]

### Added

- GitHub: `github:owner/repo@<tag-prefix>` installs one release stream of a monorepo that tags
  several addons separately (e.g. an addon and its voice packs). Each prefix is its own package in
  the lock file. Plain `owner/repo` resolution is unchanged.

### Fixed

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

