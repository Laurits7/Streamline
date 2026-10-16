# Developing

You need Rust (1.88 or newer) and Node 22 or newer.

```sh
make dev-server   # Rust API on :3000 (data in ./data)
make dev-web      # Vite on :5173 with hot reload, proxying /api to :3000
make test         # Rust tests + Svelte type check
make check        # fmt + clippy + svelte-check
make types        # regenerate TypeScript types from Rust
```

## Layout

```
crates/domain   pure logic (day boundaries, recurrence, rollover, planner…), unit-tested
crates/server   Axum HTTP API, auth, SSE, background jobs, embedded web app
crates/desktop  Linux desktop app (Tauri 2; its own Cargo workspace and lock file)
migrations      SQLite schema
web             Svelte 5 + Vite single-page app
packaging       .desktop file, .deb and tarball scripts, AUR PKGBUILDs
docs            this site, plus the spec, work plan and decisions
```

## Desktop app

The desktop app needs `webkit2gtk-4.1` and `libayatana-appindicator` (Debian/Ubuntu:
`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev`). `make desktop` builds it into
`crates/desktop/target/release/streamline-desktop`; `cargo test --manifest-path
crates/desktop/Cargo.toml` runs its tests (offline sync against a real in-process server).

!!! warning "Two lock files"
    `crates/desktop` has its own `Cargo.lock`. After changing the server's dependencies,
    refresh it (`cargo metadata --manifest-path crates/desktop/Cargo.toml --format-version 1`)
    and commit it: CI builds the desktop app with `--locked`.

## Documentation

This site is built with [MkDocs](https://www.mkdocs.org/) and
[Material](https://squidfunk.github.io/mkdocs-material/):

```sh
python3 -m venv .venv && .venv/bin/pip install -r docs/requirements.txt
.venv/bin/mkdocs serve      # http://127.0.0.1:8000, reloads on change
.venv/bin/mkdocs build --strict
```

The site is published to GitHub Pages from `main` by the *Docs* workflow.

## Where decisions live

- [Specification](../SPEC.md): what Streamline should do.
- [Work plan](../WORKPLAN.md): phases and chunks, ticked off as they land.
- [Decisions](../DECISIONS.md): answers to open questions and deviations from the spec.
