<div align="center">
  <img src="assets/riff-logo.svg" alt="Riff logo" width="720" />

  <p><strong>Music as code. Spotify in your terminal.</strong></p>
  <p>A terminal-first music player written in Rust, built around declarative, versionable playlists.</p>

  <p>
    <a href="https://github.com/Nicolas25vlad/riff/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/Nicolas25vlad/riff/actions/workflows/ci.yml/badge.svg?branch=main" /></a>
    <img alt="Rust" src="https://img.shields.io/badge/Rust-2024-orange?logo=rust" />
    <img alt="License" src="https://img.shields.io/github/license/Nicolas25vlad/riff" />
    <img alt="Status" src="https://img.shields.io/badge/status-early%20development-yellow" />
  </p>
</div>

---

## Install

Riff is published on crates.io as `riff-music`. The installed executable is still named `riff`.

```bash
cargo install riff-music
```

Linux users can also use the installer, which provisions native audio build dependencies when needed:

```bash
curl -fsSL https://raw.githubusercontent.com/Nicolas25vlad/riff/main/install.sh | bash
```

Then verify:

```bash
riff --version
riff doctor
```

For Windows, WSL, updates, native dependencies and source builds, see [INSTALL.md](INSTALL.md).

## Quick start

Create a playlist:

```bash
riff init coding-metal.riff
```

A `.riff` file is plain text and can be committed to Git:

```riff
playlist "coding-metal" {
    track "Black Sabbath - War Pigs"
    track "Dio - Holy Diver"
    track "Metallica - Orion"
    track "Megadeth - Tornado of Souls"
}
```

Open the main Riff interface:

```bash
riff tui coding-metal.riff
```

Spotify playback requires Spotify Premium. On first authentication Riff opens Spotify authorization in your browser and caches the resulting session locally.

## Riff Workbench

The TUI is the primary Riff experience. The current Workbench includes:

- Now Playing with progress, album artwork and playback state;
- Spotify search with fuzzy matching and exact track IDs;
- playlist view backed by the `.riff` file on disk;
- synchronized lyrics when Spotify provides them;
- built-in nano-like `.riff` editor;
- keyboard and mouse playback controls;
- volume, seek, shuffle and repeat controls;
- responsive layouts and terminal artwork protocols;
- Git branch/dirty context in the interface;
- built-in Riff, Kanagawa, Catppuccin and monochrome themes.

Riff keeps the text file as the source of truth. The TUI is a visual workbench around that file, not a hidden playlist database.

## CLI

```text
riff tui <file.riff>      Open the interactive Workbench
riff init [file]          Create a starter playlist
riff doctor               Check the local environment
riff validate <file>      Validate a .riff playlist
riff inspect <file>       Print parsed playlist contents
riff play <file>          Resolve and play a .riff playlist
riff search <query>       Smart Spotify search
riff inspect-track <id>   Inspect one Spotify track
riff pick <query>         Pick a recording and print pinned DSL
riff player [uri]         Start a Spotify Connect player
riff help                 Print command help
riff version              Print the installed version
```

Search accepts `--limit N`, `--threshold 0-100` and `--exact`.

## Deterministic track IDs

Text labels are convenient, but fuzzy search can resolve to different recordings. Riff supports pinning an exact Spotify track URI:

```riff
playlist "coding-metal" {
    track "Black Sabbath - War Pigs" id="spotify:track:..."
}
```

Use `riff search`, `riff inspect-track` or `riff pick` to find and inspect recordings before pinning them.

## Music as code

Riff treats playlists more like configuration than opaque app state:

- readable and editable text files;
- Git diffs and history;
- deterministic provider IDs when desired;
- terminal-first playback and editing;
- provider abstraction kept separate from the playlist language.

The longer-term direction includes richer declarative selection rules and Terraform-like `plan` / `apply` reconciliation for playlists.

## Current roadmap

Active work is tracked in GitHub issues. Near-term areas include:

- provider-resolution cache for repeated playlist loads;
- incremental resolution for large playlists;
- playback/network resilience on weak connections;
- continued TUI hardening, layout polish and terminal-safe logging;
- decoded-PCM visualizers;
- richer queue/runtime state;
- runtime configuration and keybinding customization.

## Development

Linux / WSL:

```bash
git clone https://github.com/Nicolas25vlad/riff.git
cd riff
bash scripts/deps.sh install
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

The CI runs Linux and Windows quality gates, CLI smoke tests, package validation, Remote Lab release builds and a dedicated TUI quality gate.

## Design principles

1. **Terminal first.** The TUI is a product surface, while the CLI remains scriptable.
2. **Text files are the source of truth.** A playlist should remain understandable without launching Riff.
3. **Determinism where possible.** Definitions should be reproducible enough to version meaningfully.
4. **Fast startup matters.** A terminal player that feels heavy has missed the point.
5. **Keep provider concerns isolated.** The language describes music, not Spotify internals.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). For security reports, see [SECURITY.md](SECURITY.md).

## Disclaimer

Riff is an independent open-source project and is not affiliated with, endorsed by, or sponsored by Spotify AB. Spotify is a trademark of Spotify AB.

Spotify playback uses `librespot` and requires Spotify Premium. Compatibility depends on Spotify's services and upstream `librespot` behavior.

## License

Riff is released under the [MIT License](LICENSE).

<div align="center">
  <strong>Write playlists. Commit them. Play them.</strong>
</div>
