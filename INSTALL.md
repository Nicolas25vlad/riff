# Installing Riff

Riff is currently distributed straight from GitHub while the project is in early development.

## One-line install (Linux)

```bash
curl -fsSL https://raw.githubusercontent.com/Nicolas25vlad/riff/main/install.sh | bash
```

The installer checks for the basic requirements, installs Riff through Cargo, and prints the exact PATH command if `$HOME/.cargo/bin` is not available in your current shell.

For Fish, if needed:

```fish
fish_add_path $HOME/.cargo/bin
```

Verify the installation:

```bash
riff --version
riff doctor
```

## Install with Cargo

Requirements:

- Git
- Rust stable and Cargo

Install the latest `main` build:

```bash
cargo install --git https://github.com/Nicolas25vlad/riff --locked
```

Make sure Cargo's binary directory is on your `PATH`:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

For Fish:

```fish
fish_add_path $HOME/.cargo/bin
```

## First run

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

Validate it:

```bash
riff validate playlist.riff
```

Inspect the parsed playlist:

```bash
riff inspect playlist.riff
```

You can also choose the file name when initializing:

```bash
riff init coding-metal.riff
```

Riff refuses to overwrite an existing file.

## Update

Until packaged releases are available, update by reinstalling from GitHub:

```bash
cargo install --git https://github.com/Nicolas25vlad/riff --locked --force
```

You can also rerun the one-line installer. It uses `--force`, so an existing installation is replaced by the latest `main` build.

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

The current v0.1 foundation implements the playlist DSL tooling only. Spotify authentication, `librespot` playback, the Ratatui interface, album art and audio visualization are planned but are not implemented yet.
