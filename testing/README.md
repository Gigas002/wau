# Integration test environments

Fixture WoW installations for `wau`'s CLI integration tests (`wau/tests/cli.rs`), so those tests
never touch a real `Interface/AddOns` directory. Each subdirectory is one self-contained
environment: a fake install tree plus its own `config.toml`/`profile.toml`, wired together with
relative paths (`[paths].cache = "cache"`, `profile.toml`'s `path = "_retail_/Interface/AddOns"`,
etc.) that only resolve correctly when the process's working directory is set to that
environment's own directory.

- `retail/` — `_retail_` install dir, flavour auto-detected as `mainline`.
- `classic-era/` — `_classic_era_` install dir, flavour auto-detected as `vanilla_classic`.
- `classic-mists/` — `_classic_` install dir, flavour auto-detected as `mists_classic` (the
  current `Flavour::CLASSIC` alias).
- `custom-server/` — non-Blizzard install dir name (`MyPrivateServer`), flavour can't be
  auto-detected so `profile.toml` forces it — exercises the private-server /
  `InstalledProduct::Overridden` path.
- `unconfigured/` — a bare `_retail_` install dir with no `config.toml`/`profile.toml` at all,
  for exercising the "profile isn't configured yet" error path and `wau init`'s interactive
  bootstrap.

Each `Interface/AddOns` dir only holds a `.gitkeep` (git doesn't track empty dirs) — populate it
with fixture `.toc` folders per test as needed.

## Usage

`wau/tests/cli.rs` copies the relevant environment directory into a `tempfile::tempdir()` before
invoking the compiled `wau` binary against it (`std::process::Command`, working directory set to
the copy's root), rather than running in place. Installs, DB writes, and cache files land
relative to the process's working directory, so running directly against this checked-in tree
would dirty `git status` on every test run.

Those tests are excluded from CI (`#[ignore]` on every one — CI's `cargo test --all-features`
doesn't pass `--include-ignored`) since spawning the real binary and shelling out to the
filesystem is slower and more environment-sensitive than the unit suites CI already runs. Run
them locally with:

```sh
cargo test -p wau --test cli -- --ignored
```

## `toc/`

Unrelated to the environments above: raw `.toc` files (`libwau/src/toc/tests.rs`'s
`include_str!` fixtures) exercising the `.toc`-format parser itself, not a fake install tree —
each one is a single manifest covering a different parsing case (simple, multi-client interface
lines, flavor-specific keys, CurseForge-style provider fields, color-code stripping,
dependencies). Lives here rather than under `examples/` since these aren't user-facing example
content, just test input.
