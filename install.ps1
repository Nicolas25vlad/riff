$ErrorActionPreference = "Stop"

$Repo = "https://github.com/Nicolas25vlad/riff"

function Write-Step([string]$Message) {
    Write-Host $Message -ForegroundColor Cyan
}

function Fail([string]$Message) {
    Write-Error $Message
    exit 1
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Fail "Cargo was not found. Install Rust with rustup first, then re-run this installer."
}

if (-not (Get-Command git -ErrorAction SilentlyContinue)) {
    Fail "Git was not found. Install Git for Windows first, then re-run this installer."
}

Write-Step "Installing Riff from GitHub..."
cargo install --git $Repo --locked --force

$CargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $HOME ".cargo" }
$BinDir = Join-Path $CargoHome "bin"

if (-not (Get-Command riff -ErrorAction SilentlyContinue)) {
    Write-Warning "Riff was installed to $BinDir, but that directory is not currently in PATH."
    Write-Host "Add it to your user PATH, then open a new terminal."
} else {
    Write-Step "Riff installed successfully."
    riff --version
}

Write-Host ""
Write-Host "Try it:"
Write-Host "  riff doctor"
Write-Host "  riff init"
Write-Host "  riff tui playlist.riff"
