# Releasing Riff

Riff releases are tag-driven.

A tag such as `v0.7.0` triggers `.github/workflows/release.yml`, which validates the version, builds Linux and Windows binaries, packages checksums and creates the GitHub Release. Publishing that GitHub Release then triggers `.github/workflows/publish-packages.yml`, which publishes the matching `riff-music` version to crates.io.

## Current package targets

### crates.io

Active.

The application is called **Riff** and the installed executable is still `riff`, but the crates.io package is named `riff-music` because the `riff` package name is already owned by an unrelated crate.

```bash
cargo install riff-music
riff --version
```

Upgrade with:

```bash
cargo install riff-music --force
```

### AUR

Temporarily paused because new AUR account creation is currently unavailable. The `packaging/aur/PKGBUILD.template` remains in the repository so AUR publishing can be re-enabled later, but no active CI or release workflow currently publishes to AUR.

## One-time crates.io setup

The repository needs the Actions secret:

```text
CARGO_REGISTRY_TOKEN
```

The token must belong to a verified crates.io account with permission to publish `riff-music`. Never commit it to the repository.

## Release checklist

1. Make sure `main` is green.
2. Update the version in `Cargo.toml` using SemVer.
3. Add the matching section to `CHANGELOG.md`.
4. Merge the release-preparation PR.
5. Create and push a Git tag matching the manifest exactly:

```text
Cargo.toml version = 0.7.0
Git tag            = v0.7.0
```

That tag is the only manual release trigger.

The `Release` workflow then:

- rejects tags that do not match `Cargo.toml`;
- builds optimized binaries on Linux and Windows;
- packages `.tar.gz` / `.zip` archives and SHA-256 checksum files;
- creates the GitHub Release using the matching `CHANGELOG.md` section.

The resulting GitHub Release triggers `Publish Packages`, which performs a final `cargo publish --dry-run` before publishing to crates.io.

## What CI validates before release

`Package Check` validates `cargo publish --dry-run` on packaging-related pull requests.

Normal CI owns formatting, Clippy, unit tests, cross-platform checks, CLI smoke tests, the TUI quality gate and the release-workflow contract check.

Remote Lab builds testable Linux and Windows release binaries for feature/fix branches.

## Versioning

While Riff remains in `0.x`:

- patch releases cover fixes, compatible polish, documentation and release engineering;
- minor releases cover meaningful new user-facing capabilities or larger Workbench/runtime changes.

Never move an already published release tag. If a release needs a correction, bump the version.

## Dormant AUR packaging

Do not hand-maintain generated `PKGBUILD` or `.SRCINFO` files in this repository. The dormant source template remains:

```text
packaging/aur/PKGBUILD.template
```

When AUR publishing is re-enabled, restore CI validation and generate `.SRCINFO` with Arch's own tooling before pushing.

## Current reproducibility note

Riff does not yet commit `Cargo.lock` (tracked in #13). Until that is fixed, crate builds resolve dependency versions during packaging. After #13 lands, release/package commands should be tightened to use `--locked` where supported.

## Failure behavior

A failed crates.io publish does not rewrite or delete an existing crates.io version. crates.io versions are immutable.

If the crates.io credential is missing, the publishing job exits with an explicit error instead of silently skipping publication.
