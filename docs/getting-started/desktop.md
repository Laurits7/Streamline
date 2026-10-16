# Desktop app (Linux)

Streamline in its own window, for Linux desktops (x86_64). It works in one of two ways:

- **Connected to your Streamline server** (the address you open in the browser). It signs in
  once and keeps a per-computer key in the system keyring, keeps working offline, and sends
  changes when the server is back.
- **On this computer only**, with the server inside the app and your data in
  `~/.local/share/io.github.laurits7.streamline/server`. You can move that data to a server
  later.

It stays in the tray when closed, can start at login, and can mute the speakers and show the
focus timer on every screen. *Help → Desktop app* in the app has the details.

## Install

=== "Arch Linux (AUR)"

    `streamline-desktop-bin` (the release build) or `streamline-desktop` (built from source):

    ```sh
    yay -S streamline-desktop-bin
    ```

=== "Debian, Ubuntu"

    Download `streamline-desktop_<version>_amd64.deb` from the
    [release](https://github.com/Laurits7/Streamline/releases), then:

    ```sh
    sudo apt install ./streamline-desktop_<version>_amd64.deb
    ```

=== "Other distributions"

    The release's `streamline-desktop-<version>-x86_64-linux.tar.gz` holds `usr/` to unpack into
    `/` or `~/.local`. It needs WebKitGTK 4.1, GTK 3 and libayatana-appindicator; muting needs
    `pactl` (PipeWire or PulseAudio).

## Run it

Start **Streamline** from your application menu, or run `streamline-desktop`. On first start it
asks how to use it:

- **Connect to a server**: enter your Streamline server's address (the one you open in the
  browser) and sign in once. The computer keeps its own key in the system keyring; changes made
  offline are sent when the server is back.
- **This computer only**: no server needed. Routines, the end of the day and reminders run
  while the app is running (keep it in the tray), and backups are made daily into the data
  folder.

Turn on *Start at login* in **Settings → Desktop app**.

### Moving to a server later

Quit the app and copy the data folder (**Settings → Desktop app → Open the data folder**) to the
new server as its `data` folder *before* starting the server for the first time. Then start it
and connect the app to it (*Use a server instead*). Your account there is called `me`: set its
password first, in **Settings → Desktop app**. Merging into a server that already has your data
isn't supported.

## Build it from source

Install `libwebkit2gtk-4.1-dev libayatana-appindicator3-dev` (Debian and Ubuntu names; on Arch
`webkit2gtk-4.1 libayatana-appindicator`), Rust and Node, then:

```sh
make desktop
crates/desktop/target/release/streamline-desktop
```

!!! note "Window managers"
    The corner timer is see-through only with a compositing window manager; window placement
    and always-on-top are reliable on X11 and may be ignored by Wayland compositors.
