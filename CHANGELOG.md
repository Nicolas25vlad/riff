# Changelog

All notable user-facing changes to Riff are documented here.

## 0.6.3

### Changed

- refreshed the README around the current Workbench-first product experience;
- documented the main views, controls, deterministic track workflow and current project status more clearly;
- added crates.io and latest-release badges so install/version state is visible from the repository front page;
- prepared a permanent tag-driven GitHub Release workflow with Linux and Windows release binaries.

### Release engineering

- release tags are validated against `Cargo.toml` before publishing;
- tagged releases build and package Linux and Windows binaries before creating the GitHub Release;
- the existing package publisher remains responsible for publishing the matching `riff-music` version to crates.io after the GitHub Release is published.

## 0.6.2

### Changed

- refreshed README and installation docs around the current Workbench and crates.io package;
- exposed the primary `riff tui <file>` command in `riff help`;
- added a regression test so the Workbench command cannot silently disappear from CLI help;
- added this project changelog as the release-history source of truth.

## 0.6.1

### Fixed

- isolated Workbench rendering from CLI/librespot stdout and stderr logging;
- made mouse-wheel behavior contextual so unrelated scrolling no longer changes volume;
- removed residual terminal output that could corrupt the full-screen TUI;
- preserved correct Now Playing metadata for tracks started directly from Search;
- prevented Search writes from overwriting unsaved editor changes.

### Quality

- added a dedicated TUI quality gate to CI;
- added Workbench input/hitbox regression tests;
- Remote Lab validates release builds and smoke tests on Linux and Windows;
- refreshed installation and usage documentation for the crates.io package.

## 0.6.0

### Added

- Riff Workbench v2 with Now Playing, Search, Playlist, Lyrics and Editor views;
- album artwork in supported terminals with Unicode fallback;
- Spotify search with fuzzy ranking and deterministic provider IDs;
- synchronized lyrics when available from Spotify;
- built-in nano-like `.riff` editor;
- keyboard and mouse playback controls, seek, volume, shuffle and repeat;
- Git branch/dirty context in the TUI;
- built-in Riff, Kanagawa, Catppuccin and monochrome themes;
- crates.io distribution as `riff-music` while keeping the executable name `riff`.

### Changed

- `.riff` files remain the source of truth while the TUI acts as a visual workbench around them;
- playback, artwork and lyrics share the authenticated player session where practical.

## Notes

Riff is still in early development. Spotify playback requires Spotify Premium and depends on upstream `librespot` compatibility.
