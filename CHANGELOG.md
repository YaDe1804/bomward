# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Repository foundation: public docs, ADRs 0001–0007, and the Cargo workspace.
- `bomward` parses `diff <OLD> <NEW>` with `--policy` and `--format`, then
  exits 2 with `not implemented yet`. It does not review SBOMs yet.
- `bomward-core` reads CycloneDX JSON 1.4–1.7 into the normalized model
  (`read_path`, `read_bytes`), including limits, purl identity, and warnings.
- ADRs 0008 (owned CycloneDX model), 0009 (`packageurl` 0.7.1), and 0010
  (document mapping).

### Changed

- A dependency cycle is stored. `--max-depth` limits nested `components`
  and JSON container depth, not the length of a dependency chain.
- GitHub Actions runs push builds on `main` only. Pull requests are unchanged.
