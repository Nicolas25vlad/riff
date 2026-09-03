# Installing Riff

Riff is currently distributed straight from GitHub while the project is in early development.

## One-line install (Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/Nicolas25vlad/riff/main/install.sh | bash
```

The installer checks the Rust/Git requirements and provisions the native Linux audio build dependencies on Arch/Omarchy, Debian/Ubuntu, and Fedora-family systems when they are missing.

For Fish, if needed:

```fish
fish_add_path $HOME/.cargo/bin
```

Verify the installation:

```bash
riff --version
riff doctor
```

## Linux audio dependencies

Riff's first playback backend uses `librespot` with Rodio. On Linux this builds against ALSA.

Manual packages:

```bash
# Arch / Omarchy
sudo pacman -S --needed base-devel alsa-lib pkgconf

# Debian / Ubuntu
sudo apt-get install build-essential libasound2-dev pkg-config

# Fedora
sudo dnf install gcc make alsa-lib-devel pkgconf-pkg-config
```

The one-line installer handles these automatically when necessary.

## Install with Cargo

Requirements:

- Git
- Rust stable and Cargo
- Linux audio build dependencies listed above

Install the latest `main` build:

```bash
cargo install --git https://github.com/Nicolas25vlad/riff
```

Make sure Cargo's binary directory is on your `PATH`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

For Fish:

```fish
fish_add_path $HOME/.cargo/bin
```

## Start the Spotify player

Spotify playback requires a Spotify Premium account because Riff uses `librespot`.

Start Riff as a local Spotify Connect device:

```bash
riff player
```

On the first run, Riff opens Spotify authorization in your browser. After authentication, the session is cached under your XDG cache directory (normally `~/.cache/riff/spotify`) so you do not need to log in every time.

Once the terminal prints that the player is online, select **Riff** from Spotify Connect on any Spotify client connected to your account.

You can also ask Riff to load a Spotify context URI when it starts:

```bash
riff player 'spotify:album:1ATL5GLyefJaxhQzSPVrLX'
```

Press `Ctrl+C` to stop the headless player.

## Playlist DSL

Create a starter playlist:

```bash
riff init
```

That creates `playlist.riff` in the current directory:

```riff
playlist "my-playlist" {
    track "Black Sabbath - War Pigs"
    track "Dio - Holy Diver"
}
```

Validate and inspect it:

```bash
riff validate playlist.riff
riff inspect playlist.riff
```

You can also choose the file name when initializing:

```bash
riff init coding-metal.riff
```

Riff refuses to overwrite an existing file.

The bridge from textual `.riff` tracks to Spotify search/queue playback is the next player milestone. For now, `.riff` tooling and the Spotify Connect player are separate surfaces.

## Update

Until packaged releases are available, update by reinstalling from GitHub:

```bash
cargo install --git https://github.com/Nicolas25vlad/riff --force
```

You can also rerun the one-line installer. It uses `--force`, so an existing installation is replaced by the latest `main` build.

A versioned root `Cargo.lock` and fully locked installs are tracked in issue #13. The known `librespot 0.8.0`/`vergen` resolution problem is pinned explicitly in `Cargo.toml` in the meantime.

## Uninstall

```bash
cargo uninstall riff
```

## Build from source

```bash
git clone https://github.com/Nicolas25vlad/riff.git
cd riff
cargo build --release
```

The resulting binary will be available at:

```text
target/release/riff
```

## Current limitations

The v0.2 player foundation provides local Spotify Connect playback and the existing playlist DSL tools. Direct `.riff` resolution, queue control, the Ratatui interface, album artwork and audio-reactive visualization are tracked as subsequent milestones.
