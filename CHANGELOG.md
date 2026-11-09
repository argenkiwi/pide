# Changelog

All notable changes are recorded here. The format follows [Keep a Changelog](https://keepachangelog.com), and the project uses [semantic versioning](https://semver.org).

## [Unreleased]

### Changed
- Removed the `ambler` git dependency; the walk engine (`Node`, `Transition`, `Walk`) now lives in `src/engine.rs`.

## [0.1.0]

### Added
- First standalone release, extracted from the `pide` walk in ambler-rs: containerised Pi agent backed by a local LLM, configured under `.pide/`.
