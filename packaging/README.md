# Packaging

Riff package-manager integrations live here.

Current targets:

- `aur/` — Arch User Repository package metadata

Publishing orchestration lives in `.github/workflows/publish-packages.yml` and is triggered only by a published, non-prerelease GitHub Release.

See [`RELEASING.md`](../RELEASING.md) for the release flow and required repository secrets.
