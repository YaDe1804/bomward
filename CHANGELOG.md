# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Repository foundation: public docs, ADRs 0001–0007, and the Cargo workspace.
- `bomward` parses `diff <OLD> <NEW>` with `--policy` and `--format`, then
  exits 2 with `not implemented yet`. It does not review SBOMs yet.
