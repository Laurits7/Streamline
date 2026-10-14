#!/bin/sh
# Lay out the desktop app as it's installed (usr/bin, the .desktop file, icons, licence)
# under DEST, from a release build. Used for the release tarball, the .deb and both AUR
# packages (D-83).
#   packaging/linux/stage.sh DEST [BINARY]
set -eu
dest=$1
root=$(cd "$(dirname "$0")/../.." && pwd)
bin=${2:-$root/crates/desktop/target/release/streamline-desktop}
install -Dm755 "$bin" "$dest/usr/bin/streamline-desktop"
install -Dm644 "$root/packaging/linux/streamline-desktop.desktop" "$dest/usr/share/applications/streamline-desktop.desktop"
for size in 32 128 256 512; do
  install -Dm644 "$root/crates/desktop/icons/${size}x${size}.png" "$dest/usr/share/icons/hicolor/${size}x${size}/apps/streamline-desktop.png"
done
install -Dm644 "$root/LICENSE" "$dest/usr/share/licenses/streamline-desktop/LICENSE"
