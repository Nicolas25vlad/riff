<div align="center">
  <img src="assets/riff-logo.svg" alt="Riff logo" width="720" />

  <p><strong>Music as code. Spotify in your terminal.</strong></p>
  <p>A terminal-first music player written in Rust, designed around declarative, versionable playlists.</p>

  <p>
    <a href="https://github.com/Nicolas25vlad/riff/actions/workflows/ci.yml?query=branch%3Amain"><img alt="CI" src="https://github.com/Nicolas25vlad/riff/actions/workflows/ci.yml/badge.svg?branch=main" /></a>
    <img alt="Rust" src="https://img.shields.io/badge/Rust-2024-orange?logo=rust" />
    <img alt="License" src="https://img.shields.io/github/license/Nicolas25vlad/riff" />
    <img alt="Status" src="https://img.shields.io/badge/status-early%20development-yellow" />
  </p>
</div>

---

## Install

On Linux, install the latest build from `main` in one command:

```bash
curl -fsSL https://raw.githubusercontent.com/Nicolas25vlad/riff/main/install.sh | bash
```

Then:

```bash
riff doctor
riff init
riff validate playlist.riff
riff inspect playlist.riff
```

> Using Fish and `riff` is not found after installation? Run `fish_add_path $HOME/.cargo/bin` once.

For manual installation, updates, uninstall instructions and building from source, see [INSTALL.md](INSTALL.md).

## What is Riff?

Riff is an experiment in treating your music library the way developers treat infrastructure and configuration.

Instead of building every playlist by clicking through a GUI, you describe it in a small text format, keep it beside your dotfiles, review changes with `git diff`, share it on GitHub, and eventually let Riff resolve and play it through Spotify.

```riff
playlist "coding-metal" {
    track "Black Sabbath - War Pigs"
    track "Dio - Holy Diver"
    track "Metallica - Orion"
    track "Megadeth - Tornado of Souls"
}
```

```console
$ riff validate examples/coding-metal.riff
✓ coding-metal is valid (4 tracks)

$ riff inspect examples/coding-metal.riff
coding-metal
4 tracks

  1. Black Sabbath - War Pigs
  2. Dio - Holy Diver
  3. Metallica - Orion
  4. Megadeth - Tornado of Souls
```

The long-term goal is a complete terminal music experience powered by Rust, `librespot`, Ratatui and a declarative playlist engine.

## Why?

- **Playlists as source code**: readable text files with deterministic ordering.
- **Git-native music libraries**: branch, diff, review, fork and share playlists like code.
- **Terminal-first playback**: search, queue, play, pause and navigate without leaving your shell.
- **Native Spotify playback**: `librespot` is planned as the playback and Spotify Connect engine.
- **Rich terminal UI**: album artwork, queue management, playback state and audio visualization.
- **Provider-independent core**: Spotify first, without permanently coupling the language to Spotify.

## Vision

```text
┌──────────────────────────────────────────────────────────────┐
│ RIFF                                              03:21 / 07:57│
├──────────────────────┬───────────────────────────────────────┤
│                      │ Black Sabbath                         │
│    [ album art ]     │ War Pigs                              │
│                      │ Paranoid                              │
│                      │                                       │
│                      │ ▁▂▃▅▇▅▃▂▂▃▆██▆▄▃▂▁▂▅▇██▅▂           │
├──────────────────────┴───────────────────────────────────────┤
│ Queue                                                        │
│ > War Pigs                                                   │
│   Holy Diver                                                 │
│   Orion                                                      │
│   Tornado of Souls                                           │
├──────────────────────────────────────────────────────────────┤
│ j/k move   space pause   n next   / search   : command       │
└──────────────────────────────────────────────────────────────┘
```

Planned audio path:

```text
.riff files
    │
    ▼
 parser / AST
    │
    ▼
 playlist engine ───────► provider resolution
                              │
                              ▼
                           librespot
                              │
                           decoded PCM
                           ╱         ╲
                          ▼           ▼
                    audio output   FFT/analyzer
                                      │
                                      ▼
                                  visualizer
```

## Current status

Riff is in **early development**. Today it includes:

- a dependency-free Rust CLI core;
- `riff init [file]`;
- `riff doctor`;
- `riff validate <file>`;
- `riff inspect <file>`;
- the first `.riff` playlist parser;
- parser tests;
- CI for installer syntax, formatting, Clippy, tests and `cargo check`.

Spotify playback is not implemented yet.

## Planned playlist language

The current implementation supports explicit `track` statements. The language is intended to grow toward reproducible selection rules:

```riff
playlist "night-shift" {
    add artist("Black Sabbath").top(8)
    add album("Holy Diver", by="Dio")

    add artist("Megadeth")
        .take(5)
        .shuffle(seed=666)

    exclude live
    exclude acoustic
}
```

Eventually Riff should preview changes before applying them:

```diff
coding-metal

+ Dio - Holy Diver
+ Black Sabbath - Supernaut
- Metallica - Fuel

Plan: 2 to add, 1 to remove, 4 to reorder
```

Yes, the aspiration is essentially **Terraform for playlists**, with significantly more guitar riffs.

## CLI

Available today:

```text
riff init [file]
riff doctor
riff validate <file>
riff inspect <file>
riff help
riff version
```

Planned:

```text
riff play [playlist|track]
riff pause
riff next
riff previous
riff search <query>
riff queue add <query>
riff status
riff plan
riff apply
riff tui
```

## Architecture

Riff is intentionally still a single crate while its public concepts settle. The intended evolution is roughly:

```text
riff/
├── riff-cli          command-line interface
├── riff-core         playback state, queue and domain model
├── riff-lang         lexer, parser and playlist AST
├── riff-provider     provider abstraction
├── riff-spotify      Spotify/librespot integration
├── riff-audio        PCM pipeline and audio output
├── riff-visualizer   FFT, spectrum, waveform and meters
├── riff-image        terminal image protocols and fallbacks
└── riff-tui          Ratatui application
```

| Area | Direction |
| --- | --- |
| Language | Rust |
| Spotify playback | librespot |
| Terminal UI | Ratatui + Crossterm |
| Async runtime | Tokio |
| Local state/cache | SQLite |
| Audio analysis | FFT over decoded PCM |
| Cover art | Kitty graphics / Sixel / Unicode fallback |
| Playlist language | custom parser + typed AST |

## Roadmap

### v0.1 · Foundation

- [x] Project identity and documentation
- [x] Minimal CLI
- [x] One-line Linux installer
- [x] First `.riff` parser
- [x] Playlist validation and inspection
- [x] Unit tests and CI
- [ ] Formal grammar
- [ ] Diagnostics with line/column information

### v0.2 · Spotify core

- [ ] Authentication/session management
- [ ] Track search and metadata resolution
- [ ] `librespot` playback
- [ ] Spotify Connect device mode
- [ ] Play, pause, seek, next and previous

### v0.3 · Terminal player

- [ ] Ratatui interface
- [ ] Queue browser
- [ ] Search view
- [ ] Album artwork
- [ ] Configurable keybindings

### v0.4 · Audio candy

- [ ] PCM analysis pipeline
- [ ] Spectrum visualizer
- [ ] Waveform / oscilloscope mode
- [ ] VU meter
- [ ] Theme extraction from album covers

### v0.5 · Music as code

- [ ] Expressions and reusable playlist fragments
- [ ] Deterministic shuffle seeds
- [ ] Filters and exclusions
- [ ] Imports
- [ ] Provider-independent track references
- [ ] `riff plan`
- [ ] `riff apply`

## Design principles

1. **Terminal first, not terminal only.** The CLI stays scriptable even when the TUI becomes rich.
2. **Text files are the source of truth.** A playlist should be understandable without launching Riff.
3. **Determinism where possible.** Definitions should be reproducible enough to meaningfully version.
4. **Fast startup matters.** A terminal player that feels heavy has missed the point.
5. **Keep provider concerns isolated.** The language describes music, not Spotify internals.
6. **No architecture cosplay.** Abstractions appear when they solve real problems.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). Pull requests run the full quality gate automatically, and ownership rules live in `.github/CODEOWNERS`.

For security reports, see [SECURITY.md](SECURITY.md).

## Disclaimer

Riff is an independent open-source project and is not affiliated with, endorsed by, or sponsored by Spotify AB. Spotify is a trademark of Spotify AB.

Spotify-specific functionality may require Spotify Premium and will depend on the capabilities and compatibility of `librespot` and Spotify's services at the time of implementation.

## License

Riff is released under the [MIT License](LICENSE).

<div align="center">
  <strong>Write playlists. Commit them. Play them.</strong>
</div>
