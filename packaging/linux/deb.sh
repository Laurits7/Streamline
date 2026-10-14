#!/bin/sh
# Build the .deb from a staged tree (stage.sh), with the libraries the app needs.
#   packaging/linux/deb.sh STAGED_DIR VERSION OUT_DIR
set -eu
staged=$1 version=$2 out=$3
pkg=$(mktemp -d)
cp -a "$staged/." "$pkg/"
# Debian keeps the licence as the package's copyright file.
mkdir -p "$pkg/usr/share/doc/streamline-desktop"
mv "$pkg/usr/share/licenses/streamline-desktop/LICENSE" "$pkg/usr/share/doc/streamline-desktop/copyright"
rm -r "$pkg/usr/share/licenses"
mkdir -p "$pkg/DEBIAN"
size=$(du -sk "$pkg/usr" | cut -f1)
cat > "$pkg/DEBIAN/control" <<CONTROL
Package: streamline-desktop
Version: $version
Section: utils
Priority: optional
Architecture: amd64
Installed-Size: $size
Depends: libwebkit2gtk-4.1-0, libgtk-3-0, libayatana-appindicator3-1, libc6
Recommends: pulseaudio-utils | pipewire-pulse
Maintainer: Laurits7 <30724920+Laurits7@users.noreply.github.com>
Homepage: https://github.com/Laurits7/Streamline
Description: Streamline household todo and day planner (desktop app)
 Streamline in its own window: connects to your Streamline server and works
 offline, or runs on this computer only. Tray icon, start at login, focus
 timer on every screen.
CONTROL
mkdir -p "$out"
dpkg-deb --root-owner-group --build "$pkg" "$out/streamline-desktop_${version}_amd64.deb"
rm -rf "$pkg"
