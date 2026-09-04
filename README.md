<div align="center">
  <img src="assets/riff-logo.svg" alt="Riff logo" width="720" />

  <p><strong>Music as code. Spotify in your terminal.</strong></p>
  <p>A terminal-first Spotify player built in Rust around declarative, versionable <code>.riff</code> playlists.</p>

  <p>
    <a href="https://github.com/Nicolas25vlad/riff/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/Nicolas25vlad/riff/actions/workflows/ci.yml/badge.svg?branch=main" /></a>
    <a href="https://crates.io/crates/riff-music"><img alt="crates.io" src="https://img.shields.io/crates/v/riff-music?logo=rust" /></a>
    <a href="https://github.com/Nicolas25vlad/riff/releases/latest"><img alt="GitHub release" src="https://img.shields.io/github/v/release/Nicolas25vlad/riff" /></a>
    <img alt="Rust" src="https://img.shields.io/badge/Rust-2024-orange?logo=rust" />
    <img alt="License" src="https://img.shields.io/github/license/Nicolas25vlad/riff" />
  </p>
</div>

---

## What is Riff?

Riff is a Spotify player whose main interface lives entirely in the terminal.

Instead of hiding playlists inside an application database, Riff keeps them as readable text files that can be edited, diffed and committed to Git:

```riff
playlist "coding-metal" {
    track "Black Sabbath - War Pigs"
    track "Dio - Holy Diver"
    track "Metallica - Orion" id="spotify:track:..."
    track "Megadeth - Tornado of Souls"
}
```

The TUI is a visual workbench around those files. The file remains the source of truth.

> Spotify playback requires Spotify Premium.

## Install

### Cargo

Riff is published on crates.io as `riff-music`. The installed executable is still named `riff`.

```bash
cargo install riff-music
```

Upgrade an existing Cargo installation with:

```bash
cargo install riff-music --force
```

### Linux installer

```bash
curl -fsSL https://raw.githubusercontent.com/Nicolas25vlad/riff/main/install.sh | bash
```

The installer can provision native audio build dependencies when required.

For Windows 11, WSL, source builds and troubleshooting, see [INSTALL.md](INSTALL.md).

## Quick start

```bash
riff init coding-metal.riff
riff tui coding-metal.riff
```

On first authentication, Riff opens Spotify authorization in your browser and caches the resulting session locally.

## Workbench

The Workbench is the primary Riff experience.

| View | What it does |
| --- | --- |
| **Now Playing** | album artwork, track metadata, progress and transport controls |
| **Search** | Spotify search, fuzzy ranking, exact IDs and direct playback |
| **Playlist** | shows the queue represented by the active `.riff` file |
| **Lyrics** | synchronized lyrics when Spotify provides them |
| **Editor** | built-in nano-like editor for the current `.riff` file |

The interface also includes:

- keyboard and contextual mouse controls;
- seek, volume, shuffle and repeat;
- Git branch and dirty-state context;
- responsive layouts;
- Kitty, Sixel, iTerm2 and Unicode artwork fallbacks;
- Riff, Kanagawa, Catppuccin and monochrome themes;
- terminal-safe logging rules so background output does not corrupt the TUI.

### Core controls

```text
Space        Play / pause
n / Right    Next track
p / Left     Previous track
h / l        Previous / next aliases
1..5         Switch Workbench views
F6           Cycle theme
q / Esc      Leave the Workbench
```

Mouse behavior is contextual: scrolling Search navigates results, while scrolling over the volume control changes volume. Unrelated scrolling does not alter playback.

The built-in editor follows familiar nano-style shortcuts such as `Ctrl+S` to save, `Ctrl+K` to cut a line, `Ctrl+U` to paste and `Ctrl+X` to leave the editor.

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

## Deterministic tracks

Human-readable labels are convenient, but search can resolve to different recordings. Pin an exact Spotify track URI when reproducibility matters:

```riff
playlist "coding-metal" {
    track "Black Sabbath - War Pigs" id="spotify:track:..."
}
```

Use `riff search`, `riff inspect-track` or `riff pick` to inspect recordings and generate pinned DSL.

## Music as code

Riff treats playlists closer to infrastructure configuration than opaque app state:

```text
playlist.riff
     │
     ├── readable text
     ├── Git history
     ├── deterministic provider IDs
     └── Workbench edits the same file
```

The longer-term language direction includes richer declarative selection rules and Terraform-like `plan` / `apply` reconciliation.

## Project status

Riff is usable today, but still evolving quickly.

Current near-term work includes:

- caching resolved tracks across repeated playlist loads;
- incremental resolution so large playlists can start playing immediately;
- stronger streaming resilience on unstable connections;
- continued TUI layout and interaction polish;
- decoded-PCM visualizers;
- richer queue/runtime state;
- configuration and keybinding customization.

Track active work in [GitHub Issues](https://github.com/Nicolas25vlad/riff/issues) and release history in [CHANGELOG.md](CHANGELOG.md).

## Development

Linux / WSL:

```bash
git clone https://github.com/Nicolas25vlad/riff.git
cd riff
bash scripts/deps.sh install
cargo test --all-features
cargo clippy --all-targets --all-features -- -D warnings
```

CI currently includes Linux and Windows quality gates, CLI smoke tests, package validation, Remote Lab release builds and a dedicated TUI quality gate that enforces terminal-ownership invariants.

## Design principles

1. **Terminal first.** The TUI is the primary product surface; the CLI remains scriptable.
2. **Text is the source of truth.** A playlist stays understandable without launching Riff.
3. **Determinism where possible.** Definitions should be reproducible enough to version meaningfully.
4. **Fast startup matters.** Terminal software should feel immediate.
5. **Provider concerns stay isolated.** The language describes music, not Spotify internals.

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
