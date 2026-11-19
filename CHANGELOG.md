# Changelog

## [Unreleased]

### Added

- GitHub: `github:owner/repo@<tag-prefix>` installs one release stream of a monorepo that tags
  several addons separately (e.g. an addon and its voice packs). Each prefix is its own package in
  the lock file. Plain `owner/repo` resolution is unchanged.

### Fixed

- Updating an addon didn't notice that another addon from the same source (or with the same id
  on another source) owned a folder the new version ships. If that folder was missing on disk,
  both ended up owning it; if present, the error called it "unreconciled".
- `wau replace` crashed when the new addon had a dependency, possibly after the old addon had
  already been removed. Dependencies are now installed alongside the replacement.

## [0.1.0] - 2026-10-27

