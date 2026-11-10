# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com), and the project uses [semantic versioning](https://semver.org).

## [Unreleased]

### Added
- Support for Apple's [`container`](https://github.com/apple/container) as a third container engine (after `podman` and `docker`), including its `system status` health check and JSON `image inspect` output. Bind mounts omit the SELinux `:z` suffix for it, which `container` rejects. On macOS the host must be made reachable via `host.docker.internal`, since `container` has no `--add-host` flag.

### Changed
- Engine detection now picks the first of `podman`, `docker` and `container` that is installed *and running*, instead of the first that is installed. An installed but stopped engine no longer blocks the next one; if none is running, the error still names the most-preferred installed engine.
- Removed the `ambler` git dependency; the walk engine (`Node`, `Transition`, `Walk`) now lives in `src/engine.rs`.

## [0.1.0]

### Added
- First standalone release, extracted from the `pide` walk in ambler-rs: containerised Pi agent backed by a local LLM, configured under `.pide/`.
