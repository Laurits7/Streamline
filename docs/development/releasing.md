# Releasing

1. All checks green; merge the release branch into `main`.
2. On `main`: `scripts/bump-version.sh X.Y.Z` (server, desktop app, web app and every lock file).
3. In `CHANGELOG.md`, change `## Unreleased (X.Y.Z)` to `## X.Y.Z (YYYY-MM-DD)`; the release
   notes are cut from that heading.
4. Update the pinned image example in
   [Backups and updates](../getting-started/backups-and-updates.md#updating).
5. Commit "Release X.Y.Z", push `main`, then tag: `git tag vX.Y.Z && git push origin vX.Y.Z`.
   The version bump must be in the tagged commit: the desktop job checks the app's version
   against the tag.
6. CI publishes the image; the *Release* workflow creates the GitHub release with the compose
   file and attaches the desktop `.deb` and tarball.

## AUR

Needs your AUR account and its SSH key. `packaging/aur/update.sh X.Y.Z` sets `pkgver`, the
checksums of the released files and `.SRCINFO` for both packages. Commit that, then for each of
`streamline-desktop-bin` and `streamline-desktop`:

```sh
git clone ssh://aur@aur.archlinux.org/streamline-desktop-bin.git /tmp/aur-bin   # first time
cp packaging/aur/streamline-desktop-bin/{PKGBUILD,.SRCINFO} /tmp/aur-bin/
cd /tmp/aur-bin && git add PKGBUILD .SRCINFO && git commit -m "X.Y.Z" && git push origin HEAD:master
```

The AUR only accepts the `master` branch; the first push creates the package. Check a package
locally first with `makepkg -si` in its folder and `namcap` on the result. For a packaging-only
fix, raise `pkgrel` and regenerate `.SRCINFO` with `makepkg --printsrcinfo > .SRCINFO`.
