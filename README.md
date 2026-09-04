<div align="center">
  <img src="assets/riff-logo.svg" alt="Riff logo" width="720" />

  <p><strong>Music as code. Spotify in your terminal.</strong></p>
  <p>A terminal-first music player written in Rust, built around declarative, versionable playlists.</p>

  <p>
    <a href="https://github.com/Nicolas25vlad/riff/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/Nicolas25vlad/riff/actions/workflows/ci.yml/badge.svg?branch=main" /></a>
    <img alt="Rust" src="https://img.shields.io/badge/Rust-2024-orange?logo=rust" />
    <img alt="License" src="https://img.shields.io/github/license/Nicolas25vlad/riff" />
  </p>
</div>

---

## Install

The published Cargo package is `riff-music`; the installed command is still `riff`.

```bash
cargo install riff-music
```

Or install the current GitHub build on Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/Nicolas25vlad/riff/main/install.sh | bash
```

See [INSTALL.md](INSTALL.md) for native audio dependencies, Windows installation and source builds.

Spotify playback requires Spotify Premium.

## Music as code

A `.riff` file is the source of truth:

```riff
playlist "coding-metal" {
    track "Black Sabbath - War Pigs"
    track "Dio - Holy Diver"
    track "Metallica - Orion"
    track "Megadeth - Tornado of Souls"
}
```

For deterministic playback you can pin the provider ID:

```riff
track "Black Sabbath - War Pigs" id="spotify:track:..."
```

Validate, inspect and play the same file from the terminal:

```bash
riff validate examples/coding-metal.riff
riff inspect examples/coding-metal.riff
riff play examples/coding-metal.riff
riff tui examples/coding-metal.riff
```

## Workbench TUI

`riff tui <file.riff>` is the primary interactive experience. The Workbench currently has five views:

- **Now Playing**: artwork, metadata, transport state, progress, volume, shuffle and repeat.
- **Search**: fuzzy Spotify search, preview/play and exact-ID insertion into the current `.riff` file.
- **Playlist**: the declarative queue generated from the current file.
- **Lyrics**: synchronized lyrics when Spotify exposes them.
- **Editor**: a built-in nano-like editor with parser validation before save.

The TUI owns the terminal framebuffer. Normal operation writes no logs to stdout/stderr. If `RIFF_LOG` is set, Workbench diagnostics are appended to `riff-tui.log` under Riff's platform cache directory instead of being painted over the UI.

Startup now shows a real spinner while Riff connects to Spotify and resolves playlist metadata. Resolution results are cached persistently, so reopening the same unpinned playlist can reuse already-resolved Spotify track IDs.

### Default controls

| Action | Keys |
| --- | --- |
| Next / previous view | `Tab` / `Shift+Tab` |
| Direct views | `Alt+1` … `Alt+5` |
| Play / pause | `Space` |
| Next / previous track | `n` / `p`, `l` / `h`, arrows |
| Volume | `+` / `-` in 5 percentage-point steps |
| Seek | `[` / `]` in 5 second steps |
| Shuffle / repeat | `s` / `r` |
| Search / Editor / Lyrics | `/` / `e` / `y` |
| Cycle theme | `F6` |
| Quit | `q` / `Esc` |

Mouse input is contextual: scrolling Search moves through results; scrolling over the volume gauge changes volume; unrelated areas do not unexpectedly change audio. Clicking the progress bar seeks and the transport controls are clickable.

## Spotify resolution and search

Riff uses librespot's internal Spotify search surface rather than the public Web API. Search fetches a broad candidate pool, ranks title/artist/token similarity, preserves distinct versions, and penalizes likely live/remaster/re-record/compilation variants when a canonical match is equally strong.

That ranking can only choose among candidates Spotify's internal search returns. Riff cannot discover a version that upstream search never exposes, so ambiguous titles may still benefit from `riff pick` and a pinned `spotify:track:` ID.

Repeated unpinned resolutions are stored in a versioned persistent cache. Pinned IDs always bypass fuzzy resolution.

## Useful commands

```text
riff init [file]
riff doctor
riff validate <file>
riff inspect <file>
riff play <file.riff>
riff search <query>
riff inspect-track <query-or-uri>
riff pick <query>
riff player [spotify-uri]
riff tui <file.riff>
```

## Quality gates

Riff treats terminal UX as product behavior, not just decoration. Pull requests run:

- Linux formatting, Clippy, tests, `cargo check` and CLI smoke tests;
- Windows 11 formatting, Clippy, tests, `cargo check` and CLI smoke tests;
- a dedicated **TUI quality gate** that rejects direct Workbench stdout/stderr writes, enforces terminal/logging invariants, exercises semantic input/hitboxes and renders wide/compact layouts with Ratatui's virtual `TestBackend`;
- Remote Lab release builds for Linux and Windows with downloadable artifacts;
- crates.io package dry-runs when packaging metadata changes.

`Cargo.lock` is committed and CI, installers, Remote Lab and release publishing use locked dependency resolution for reproducible application builds.

## Architecture

```text
.riff files
    │
    ▼
 parser / AST
    │
    ├──────────────► persistent resolution cache
    │                         │
    ▼                         ▼
 playlist engine ───────► Spotify resolution
                              │
                              ▼
                           librespot
                              │
                    playback / metadata / lyrics
                              │
                              ▼
                       Ratatui Workbench
```

Riff intentionally remains one Rust crate while its public concepts settle. Provider concerns stay isolated from the playlist language so future local/MPD/Navidrome-style providers do not require rewriting `.riff` syntax.

## Roadmap

Near-term work is tracked in GitHub issues. Major remaining areas include incremental resolution of very large playlists, more resilient streaming on weak connections, queue/runtime synchronization, the visualizer pipeline, richer playlist language and `plan/apply` workflows.

The long-term idea remains pleasantly unreasonable: **Terraform for playlists, with more guitar riffs.**

## Design principles

1. **The TUI is a product surface.** Terminal ownership, responsiveness and predictable controls are release criteria.
2. **Text files are the source of truth.** Visual editing must produce understandable `.riff` files.
3. **Determinism where possible.** Exact provider IDs and locked application dependencies keep important behavior reproducible.
4. **Fast startup matters.** Cache work already done and avoid blocking playback on work that can happen later.
5. **Keep provider concerns isolated.** The language describes music, not Spotify internals.
6. **No architecture cosplay.** Abstractions appear when they solve a real problem.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). For security reports, see [SECURITY.md](SECURITY.md).

## Disclaimer

Riff is an independent open-source project and is not affiliated with, endorsed by, or sponsored by Spotify AB. Spotify is a trademark of Spotify AB.

Spotify playback uses `librespot`. Compatibility depends on Spotify's services and upstream librespot behavior.

## License

Riff is released under the [MIT License](LICENSE).

<div align="center">
  <strong>Write playlists. Commit them. Play them.</strong>
</div>
