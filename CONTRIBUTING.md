# Contributing to Riff

Thanks for helping shape Riff while it is still young.

## Before coding

For parser changes, playback architecture, provider abstractions, or large UI decisions, open an issue first so behavior can be discussed before code locks in an interface.

Small fixes, tests, documentation improvements, and focused refactors can go straight to a pull request.

## Development setup

```bash
git clone https://github.com/Nicolas25vlad/riff.git
cd riff
cargo run -- doctor
```

Create a focused branch:

```bash
git checkout -b feat/my-change
```

Before opening a PR, run:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo check --all-targets --all-features
```

## Pull requests

Keep PRs small enough to review comfortably. Explain the behavior change, not just the files changed.

When behavior changes, add tests. When user-facing commands or syntax change, update the docs in the same PR.

Prefer conventional, focused commit messages such as:

```text
feat: add playlist imports
fix: reject malformed track statements
docs: clarify cargo installation
chore: update CI cache action
```

## Design principles

Riff favors simple interfaces, fast startup, deterministic behavior, readable text formats, and provider isolation. Avoid adding abstractions before they solve a concrete problem.

## Security

Do not put Spotify credentials, session data, tokens, or other secrets in issues, commits, examples, logs, or screenshots.
