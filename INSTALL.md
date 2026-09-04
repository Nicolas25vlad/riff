# Installing Riff

Riff is published on crates.io as `riff-music`. The installed executable is named `riff`.

## Cargo

If Rust/Cargo is already installed, this is the preferred installation path on Linux, WSL and Windows:

```bash
cargo install riff-music
```

Verify:

```bash
riff --version
riff doctor
```

If Fish cannot find the binary after installation:

```fish
fish_add_path $HOME/.cargo/bin
```

## Linux and WSL installer

The repository installer can provision the native audio build dependencies and install the current GitHub build:

```bash
curl -fsSL https://raw.githubusercontent.com/Nicolas25vlad/riff/main/install.sh | bash
```

It supports Arch/Omarchy, Debian/Ubuntu, Fedora-family systems and WSL distributions based on them.

### WSL audio

On WSL2 with WSLg, Riff detects the environment and prefers its PulseAudio backend when `PULSE_SERVER` is available. WSLg forwards that audio to Windows.

You normally do not need to configure audio manually. To inspect the environment:

```bash
printf '%s\n' "$WSL_DISTRO_NAME" "$PULSE_SERVER"
```

To override automatic audio selection intentionally:

```bash
RIFF_AUDIO_BACKEND=rodio riff tui playlist.riff
```

## Windows 11 native

Requirements for building/installing through Cargo:

- Rust stable with Cargo;
- a working MSVC Rust toolchain/build environment.

Install from crates.io:

```powershell
cargo install riff-music
```

The repository PowerShell installer is also available:

```powershell
irm https://raw.githubusercontent.com/Nicolas25vlad/riff/main/install.ps1 | iex
```

Riff uses librespot's Rodio backend on Windows and outputs through the native Windows audio stack. Spotify credentials/cache are stored under `%LOCALAPPDATA%\Riff\spotify` when available.

Verify:

```powershell
riff --version
riff doctor
riff tui playlist.riff
```

## Linux / WSL audio dependencies

When building locally, Riff compiles Rodio plus PulseAudio support on Linux. Manual packages:

```bash
# Arch / Omarchy
sudo pacman -S --needed base-devel alsa-lib libpulse pkgconf

# Debian / Ubuntu / WSL Ubuntu
sudo apt-get install build-essential libasound2-dev libpulse-dev pkg-config

# Fedora
sudo dnf install gcc make alsa-lib-devel pulseaudio-libs-devel pkgconf-pkg-config
```

From a cloned Linux/WSL checkout you can let Riff manage these checks:

```bash
bash scripts/deps.sh install
```

## First run

Spotify playback requires Spotify Premium because Riff uses `librespot`.

Create a playlist:

```bash
riff init playlist.riff
```

Open the Workbench:

```bash
riff tui playlist.riff
```

On first authentication, Riff opens Spotify authorization in the browser and caches the resulting credentials locally.

Headless Spotify Connect mode is also available:

```bash
riff player
```

## Playlist DSL

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

For crates.io installations:

```bash
cargo install riff-music --force
```

If you intentionally installed the GitHub build instead, rerun the platform installer or use:

```bash
cargo install --git https://github.com/Nicolas25vlad/riff --force
```

## Uninstall

For the crates.io package:

```bash
cargo uninstall riff-music
```

Older Git installations may be registered under the old package name. Check with:

```bash
cargo install --list | grep -A3 -B1 riff
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
