# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.0] - 2020-03-26

The first release after forking from Axiom.

### Added

- Create new `ActorSystem::spawn_pool` function for spawning actor pools and returning an `AidPool` implementation.
    - Add traits: `AidPool`, `SyncAidPool`
    - Add `AidPool` implementations: `RandomAidPool`, `Aid` ( the existing `Aid` type now implements `AidPool` )
    - Add `actor-pool` Cargo feature that is enabled by default and gates off the `RandomAidPool` implementation to avoid bringin in extra dependencies.

[unreleased]: https://github.com/katharostech/maxim/compare/v0.1.0-alpha.0...HEAD
[0.1.0-alpha.0]: https://github.com/katharostech/maxim/releases/tag/v0.1.0-alpha.0