#!/bin/sh
# Point both AUR packages at a released version: pkgver, pkgrel=1, checksums of the
# release files and .SRCINFO. Run after the release workflow has published VERSION.
#   packaging/aur/update.sh 1.3.0
set -eu
version=$1
cd "$(dirname "$0")"
for pkg in streamline-desktop-bin streamline-desktop; do
  (
    cd "$pkg"
    sed -i -e "s/^pkgver=.*/pkgver=$version/" -e "s/^pkgrel=.*/pkgrel=1/" PKGBUILD
    updpkgsums
    # Never publish without real checksums (the repo copies say SKIP until a release).
    if grep -q "SKIP" PKGBUILD; then echo "$pkg: checksums missing" >&2; exit 1; fi
    makepkg --printsrcinfo > .SRCINFO
    rm -f ./*.tar.gz
  )
done
git diff --stat .
