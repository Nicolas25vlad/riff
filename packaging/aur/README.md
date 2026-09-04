# AUR packaging

The checked-in source of truth is `PKGBUILD.template`.

`PKGBUILD` and `.SRCINFO` are generated during CI/release and intentionally ignored in this repository.

On a published GitHub Release, `.github/workflows/publish-packages.yml`:

1. reads the release version;
2. downloads the tagged GitHub source archive;
3. calculates its SHA-256;
4. renders `PKGBUILD`;
5. generates `.SRCINFO` with `makepkg --printsrcinfo` in Arch Linux;
6. pushes the generated files to `ssh://aur@aur.archlinux.org/riff.git`.

See [`RELEASING.md`](../../RELEASING.md) for credential/bootstrap instructions.
