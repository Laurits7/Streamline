# HTTPS, phone app and notifications

Plain HTTP on your LAN works for everything except installing Streamline as an app, push
notifications and GPS places, which browsers only allow over HTTPS. Put it behind a proxy that
provides HTTPS and set `TRUST_PROXY: "true"` (and optionally `PUBLIC_URL`).

!!! note "Don't buffer"
    Live sync uses server-sent events, so the proxy must not buffer responses.

## Proxies

=== "Tailscale"

    Private, nothing exposed to the internet: on the host, run

    ```sh
    tailscale serve --bg 3000
    ```

    This gives `https://<machine>.<tailnet>.ts.net` with a valid certificate on every device
    in your tailnet. Set `TRUST_PROXY: "true"`. It's the easiest way to get notifications on
    your phone.

=== "Caddy"

    Automatic certificates:

    ```
    todo.example.org {
        reverse_proxy streamline:3000 {
            flush_interval -1   # deliver live-sync events immediately
        }
    }
    ```

    A complete, tested setup (Streamline + Caddy, public or internal certificates) is in
    [`docs/examples/caddy`](https://github.com/Laurits7/Streamline/tree/main/docs/examples/caddy).

=== "Traefik"

    Labels on the `streamline` service; assumes an existing Traefik with a `websecure`
    entrypoint and a certificate resolver named `le`:

    ```yaml
        labels:
          - traefik.enable=true
          - traefik.http.routers.streamline.rule=Host(`todo.example.org`)
          - traefik.http.routers.streamline.entrypoints=websecure
          - traefik.http.routers.streamline.tls.certresolver=le
          - traefik.http.services.streamline.loadbalancer.server.port=3000
    ```

    Traefik streams server-sent events without extra settings.

## Install it on your phone

Over HTTPS, Streamline can be installed as an app and send notifications even when it's closed:

- **iPhone/iPad (iOS 16.4+):** open it in Safari, tap **Share → Add to Home Screen**, open it
  from the Home Screen, then **Settings → Notifications → Turn on**.
- **Android / desktop Chrome, Edge:** use the browser's *Install app* menu (optional), then
  **Settings → Notifications → Turn on**.

Offline, an installed app shows the data it last loaded (read-only).

## Notifications

Notifications go through the browser maker's push service (Apple, Google, Mozilla), so the
server needs outgoing internet access; it doesn't need to be reachable from the internet. The
push signing key is generated as `data/vapid.key`; keep it with your backups, otherwise devices
just have to turn notifications on again.

Without HTTPS, the optional [ntfy](https://ntfy.sh) channel (**Settings → Notifications**) still
delivers notifications.
