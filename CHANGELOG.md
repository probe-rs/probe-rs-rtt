# Changelog

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

### Changed

### Fixed

## [0.10.1]

### Fixed

- Fixed a bug where RTT pointers could be torn because of 8bit reads instead of 32bit reads.

## [0.10.0]

### Changed

- Updated to probe-rs 0.10.0

## [0.4.0]

### Added

- Added more logs on all levels.

### Changed

### Fixed

- Fixed a bug where RTT would deadlock.

## [0.3.0]

### Added

- Added a proper warning if no RTT channels are found to be configured.

### Changed

### Fixed

- Fixed some error in the docs.

[Unreleased]: https://github.com/probe-rs/probe-rs/compare/v0.10.1...master
[0.10.1]: https://github.com/probe-rs/probe-rs/compare/v0.10.0...v0.10.1
[0.10.0]: https://github.com/probe-rs/probe-rs/compare/v0.4.0...v0.10.0
[0.4.0]: https://github.com/probe-rs/probe-rs/compare/v0.3.0...v0.4.0
[0.3.0]: https://github.com/probe-rs/probe-rs/releases/tag/v0.3.0
