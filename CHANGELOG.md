# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com), and the project uses [semantic versioning](https://semver.org).

## [Unreleased]

### Changed
- The `/containerfile` prompt asks Pi to make the project's dev-dependency tools runnable (such as an Ambler project's `cargo ambler`, `bun run ambler` or `uv run ambler`), so their guides and scaffolding work in the container. Existing prompts are left untouched; delete `.pide/pi/prompts/containerfile.md` to get the new one.
- Removed the `ambler` git dependency; the walk engine (`Node`, `Transition`, `Walk`) now lives in `src/engine.rs`.

## [0.1.0]

### Added
- First standalone release, extracted from the `pide` walk in ambler-rs: containerised Pi agent backed by a local LLM, configured under `.pide/`.
