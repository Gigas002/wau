# Changelog

## [Unreleased]

### Added

- GitHub: `github:owner/repo@<tag-prefix>` installs one release stream of a monorepo that tags
  several addons separately (e.g. an addon and its voice packs). Each prefix is its own package in
  the lock file. Plain `owner/repo` resolution is unchanged.

### Fixed

- `wau replace` crashed when the new addon had a dependency, possibly after the old addon had
  already been removed. Dependencies are now installed alongside the replacement.

## [0.1.0] - 2026-10-27

