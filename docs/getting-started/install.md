# Install

Streamline runs as one Docker container with one data folder. Prebuilt images exist for
amd64 and arm64 (e.g. a Raspberry Pi).

## With the release's compose file

Each [release](https://github.com/Laurits7/Streamline/releases) has a `docker-compose.yml` that
uses the prebuilt image of that version. Put it in an empty directory and run:

```sh
docker compose up -d
```

## From source

```sh
git clone https://github.com/Laurits7/Streamline.git streamline && cd streamline
docker compose up -d --build
```

To use the prebuilt image with the repository's compose file instead, remove its `build:`
section and set `image: ghcr.io/laurits7/streamline:latest`.

## First run

Open `http://<server-ip>:3000`. On the first visit you create the admin account; add more
people under **Settings → Users**.

!!! warning "Reachable by others before you've set it up?"
    The first person to open a fresh install becomes the admin. If the server is reachable by
    others before you've done that, set `INITIAL_ADMIN_USER` and `INITIAL_ADMIN_PASSWORD`
    (see [Configuration](configuration.md)) so the admin is created on startup.

Everything is stored in `./data`: a single SQLite database plus its WAL files, backups and
two keys (see [Backups and updates](backups-and-updates.md)).

Plain HTTP on your home network works for everything except installing it as a phone app,
push notifications and GPS places. For those, [put it behind HTTPS](https.md).

## Raspberry Pi (arm64)

- **Prebuilt image**: use the release's compose file, or set
  `image: ghcr.io/laurits7/streamline:latest` as above.
- **Build on the Pi**: works as-is. On a model with 1–2 GB of RAM, uncomment the `args:` in
  `docker-compose.yml` (`LTO: thin`, `CODEGEN_UNITS: "16"`) so compiling fits in memory.

## Resource use

Measured for 1.5 (x86-64, `scripts/measure.sh`): a 16.7 MB image, about 9 MB of memory while
idle, and 118 KB gzipped for the first load of the web app (other pages load as needed).
