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
riff player
```

The installer provisions the native audio build dependencies on supported Linux distributions when needed. If Fish cannot find the binary after installation, run `fish_add_path $HOME/.cargo/bin` once.

For manual installation, updates, distro dependencies and building from source, see [INSTALL.md](INSTALL.md).

## What is Riff?

Riff treats your music library the way developers treat infrastructure and configuration.

Instead of building every playlist by clicking through a GUI, you describe it in a small text format, keep it beside your dotfiles, review changes with `git diff`, share it on GitHub, and let Riff grow into the runtime that resolves and plays those definitions.

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

## First playback

Riff now has the first headless Spotify playback core built on `librespot`.

Start a local Spotify Connect device:

```bash
riff player
```

On first run, Riff opens Spotify authorization in your browser and caches the resulting session under your XDG cache directory. Once it reports that the player is online, choose **Riff** from Spotify Connect.

You can also start with a Spotify context URI:

```bash
riff player 'spotify:album:1ATL5GLyefJaxhQzSPVrLX'
```

Spotify playback requires Premium. Directly resolving the textual tracks in a `.riff` file into Spotify tracks is the next provider/queue milestone, so the playlist DSL and player are still separate surfaces in this version.

## Why?

- **Playlists as source code**: readable text files with deterministic ordering.
- **Git-native music libraries**: branch, diff, review, fork and share playlists like code.
- **Terminal-first playback**: a headless player today, interactive TUI next.
- **Native Spotify playback**: `librespot` provides local audio and Spotify Connect.
- **Rich terminal UI**: album artwork, queue management, playback state and audio visualization are tracked milestones.
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

The architecture is growing toward a decoded PCM path shared by playback and visualization:

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

Riff is in **early development**, but it now crosses the line from DSL prototype into an actual player foundation:

- `riff player [spotify-uri]` starts a local Spotify Connect player;
- OAuth login with cached credentials/session data;
- local Rodio/ALSA audio output on Linux;
- XDG-compatible Spotify cache directory;
- `riff init [file]`;
- `riff doctor`;
- `riff validate <file>`;
- `riff inspect <file>`;
- the first `.riff` playlist parser and tests;
- CI for installer syntax, formatting, Clippy, tests and `cargo check`.

The next playback step is provider resolution: turning `track "Artist - Song"` into concrete Spotify IDs and feeding the Riff queue.

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

Available in the v0.2 player foundation:

```text
riff player [spotify-uri]
riff init [file]
riff doctor
riff validate <file>
riff inspect <file>
riff help
riff version
```

Planned:

```text
riff play <file.riff>
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
| Local state/cache | XDG paths, SQLite later if needed |
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

- [x] Authentication/session foundation
- [x] `librespot` playback core
- [x] Spotify Connect device mode
- [ ] Track search and metadata resolution
- [ ] Riff-owned queue and play/pause/seek/next/previous commands

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

## Development roadmap

The implementation is tracked as focused GitHub issues instead of one giant mega-PR:

- **#5** Spotify playback core
- **#6** Spotify metadata/provider resolution
- **#7** Ratatui terminal interface
- **#8** decoded-PCM visualizer pipeline
- **#9** terminal album artwork
- **#10** queue/autoplay and `.riff` playback integration
- **#11** runtime configuration and audio backend selection

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

Spotify playback uses `librespot` and requires Spotify Premium. Compatibility depends on Spotify's services and upstream `librespot` behavior.

## License

Riff is released under the [MIT License](LICENSE).

<div align="center">
  <strong>Write playlists. Commit them. Play them.</strong>
</div>
