# Releasing Riff

GitHub Releases are the source of truth for package publication.

A published, non-prerelease GitHub Release with a tag such as `v0.6.0` triggers `.github/workflows/publish-packages.yml`, which publishes the matching version to crates.io.

## Current package targets

### crates.io

Active.

```bash
cargo install riff
```

### AUR

Temporarily paused because new AUR account creation is currently unavailable. The `packaging/aur/PKGBUILD.template` is intentionally kept in the repository so AUR publishing can be re-enabled later, but no active CI or release workflow currently publishes to AUR.

## One-time crates.io setup

1. Sign in to crates.io with the GitHub account that will own the `riff` crate.
2. Verify the crates.io account email.
3. Create an API token with permission to publish/update the crate.
4. In this GitHub repository, create an Actions secret named:

```text
CARGO_REGISTRY_TOKEN
```

Never commit this token to the repository.

## Normal release checklist

1. Make sure `main` is green.
2. Update the version in `Cargo.toml` using SemVer.
3. Merge the version/release changes through a PR.
4. Create a Git tag matching the manifest exactly:

```text
Cargo.toml version = 0.7.0
Git tag            = v0.7.0
```

5. Create and **publish** a GitHub Release for that tag.
6. Watch the `Publish Packages` workflow.
7. Verify the new version on crates.io.

The workflow deliberately aborts if the release tag does not equal `v${Cargo.toml version}`.

## First crates.io bootstrap

The first publication of Riff 0.6.0 uses `.github/workflows/publish-crates-bootstrap.yml` because the connected automation used to prepare the repository cannot create a GitHub Release directly.

The bootstrap workflow:

- only triggers when that workflow itself lands on `main`;
- is pinned to version `0.6.0`;
- requires `CARGO_REGISTRY_TOKEN`;
- runs `cargo publish --dry-run` immediately before publication;
- then runs the real `cargo publish`.

After Riff 0.6.0 is verified on crates.io, remove the bootstrap workflow. Future versions use the normal GitHub Release workflow above.

## What CI validates before release

`Package Check` runs on packaging-related pull requests and validates `cargo publish --dry-run` on Linux.

The normal Riff CI still owns Rust formatting, Clippy, tests, cross-platform checks and CLI smoke tests.

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
