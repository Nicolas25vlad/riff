# Installing Riff

Riff is currently distributed straight from GitHub while the project is in early development.

## Linux and WSL

Install the latest build from `main`:

```bash
curl -fsSL https://raw.githubusercontent.com/Nicolas25vlad/riff/main/install.sh | bash
```

The installer checks Rust/Git and provisions the native audio build dependencies on Arch/Omarchy, Debian/Ubuntu, Fedora-family systems and WSL distributions based on them.

For Fish, if needed:

```fish
fish_add_path $HOME/.cargo/bin
```

Verify:

```bash
riff --version
riff doctor
riff tui playlist.riff
```

### WSL audio

On WSL2 with WSLg, Riff detects the WSL environment and prefers its compiled PulseAudio backend when `PULSE_SERVER` is available. WSLg forwards that audio to the Windows host.

You normally do not need to configure audio manually. To inspect the environment:

```bash
printf '%s\n' "$WSL_DISTRO_NAME" "$PULSE_SERVER"
```

If you intentionally want to override automatic audio selection, set `RIFF_AUDIO_BACKEND`, for example:

```bash
RIFF_AUDIO_BACKEND=rodio riff tui playlist.riff
```

## Windows 11 native

Requirements:

- Git for Windows
- Rust stable with Cargo
- a working MSVC Rust toolchain/build environment

From PowerShell:

```powershell
irm https://raw.githubusercontent.com/Nicolas25vlad/riff/main/install.ps1 | iex
```

Or install directly with Cargo:

```powershell
cargo install --git https://github.com/Nicolas25vlad/riff --force
```

Riff uses librespot's Rodio backend on Windows, which outputs through the native Windows audio stack. Spotify credentials/cache are stored under `%LOCALAPPDATA%\Riff\spotify` when available.

Verify:

```powershell
riff --version
riff doctor
riff tui playlist.riff
```

## Project dependencies

From a cloned Linux/WSL checkout:

```bash
bash scripts/deps.sh install
```

Useful commands:

```bash
bash scripts/deps.sh update
bash scripts/deps.sh check
bash scripts/deps.sh native
bash scripts/deps.sh rust
```

On Windows, use Cargo directly:

```powershell
cargo fetch
cargo check --all-targets --all-features
cargo test --all-features
```

## Linux / WSL audio dependencies

Riff compiles Rodio plus PulseAudio support on Linux. Rodio remains the normal Linux default; WSLg prefers PulseAudio automatically.

Manual packages:

```bash
# Arch / Omarchy
sudo pacman -S --needed base-devel alsa-lib libpulse pkgconf

# Debian / Ubuntu / WSL Ubuntu
sudo apt-get install build-essential libasound2-dev libpulse-dev pkg-config

# Fedora
sudo dnf install gcc make alsa-lib-devel pulseaudio-libs-devel pkgconf-pkg-config
```

## Start the player

Spotify playback requires Spotify Premium because Riff uses `librespot`.

Headless Spotify Connect device:

```bash
riff player
```

Play a `.riff` playlist with the terminal UI:

```bash
riff tui playlist.riff
```

On first authentication, Riff opens Spotify authorization in the browser and caches the resulting credentials.

## Playlist DSL

Create a starter playlist:

```bash
riff init
```

Example:

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

## Update

```bash
cargo install --git https://github.com/Nicolas25vlad/riff --force
```

You can also rerun the platform installer.

## Uninstall

```bash
cargo uninstall riff
```

## Build from source

Linux / WSL:

```bash
git clone https://github.com/Nicolas25vlad/riff.git
cd riff
bash scripts/deps.sh install
cargo build --release
```

Windows PowerShell:

```powershell
git clone https://github.com/Nicolas25vlad/riff.git
cd riff
cargo build --release
```

The binary is written under `target/release/` (`riff` on Unix-like systems, `riff.exe` on Windows).
