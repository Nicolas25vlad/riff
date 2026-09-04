Riff v0.6.3 is a documentation and release-engineering patch focused on making the project easier to install, understand and ship.

### Changed
- README now reflects the Workbench-first product, current controls and actual crates.io install path
- release/version badges expose the current published state directly from the repository front page
- release process is now tag-driven instead of relying on disposable bootstrap workflows

### Release engineering
- Linux and Windows release binaries are built from the release tag
- archives include SHA-256 checksum files
- GitHub Release notes are generated from the matching `CHANGELOG.md` section
- publishing the GitHub Release triggers the existing crates.io publisher

### Install / upgrade
```bash
cargo install riff-music --force
```

Spotify playback requires Spotify Premium.
