# Streamline

[![CI](https://github.com/Laurits7/Streamline/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Laurits7/Streamline/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Laurits7/Streamline)](https://github.com/Laurits7/Streamline/releases/latest)
[![License: GPL-3.0-or-later](https://img.shields.io/github/license/Laurits7/Streamline)](LICENSE)
[![Image](https://img.shields.io/badge/image-ghcr.io%2Flaurits7%2Fstreamline-2496ED?logo=docker&logoColor=white)](https://github.com/Laurits7/Streamline/pkgs/container/streamline)
[![Desktop app](https://img.shields.io/badge/desktop-Linux%20%28.deb%2C%20tarball%29-FCC624?logo=linux&logoColor=black)](#desktop-app-linux)

[![rustfmt](https://img.shields.io/badge/rustfmt-checked-success?logo=rust)](.github/workflows/ci.yml)
[![clippy](https://img.shields.io/badge/clippy-no%20warnings-success?logo=rust)](.github/workflows/ci.yml)
[![TypeScript](https://img.shields.io/badge/TypeScript-strict-success?logo=typescript&logoColor=white)](web/tsconfig.app.json)
[![Bundle](https://img.shields.io/badge/initial%20load-%3C150%20KB%20gzip-success)](.github/workflows/ci.yml)

A self-hosted todo list and day planner for a household. One small container, one data
folder, and it works from any phone, tablet or computer on your network.

> **Version 1.4.** Everything in [`docs/SPEC.md`](docs/SPEC.md) up to the v1.0 phase is built: day
> planning, routines, multi-step chores, household groups, your calendar, namedays and birthdays,
> time blocks, journal and tracking, goals, an installable app with notifications, and backups.
> 1.2 added waiting for results, checklists and default tasks, project ideas, project marks, a
> daily health check-in and a guided tour. 1.3 added the [Linux desktop app](#desktop-app-linux)
> (works offline, or on one computer without a server), checklist steps on the timeline and in
> the Agenda, default tasks with kinds, and focus options. 1.4 adds search and keyboard
> shortcuts, a week view, inbox processing, a weekly review, realistic estimates, a morning
> digest, printing the day, writing your plan to your calendar, tasks from GitHub, GitLab and
> Gitea, [sharing projects with other Streamline servers](#sharing-a-project-with-someone-on-another-streamline-server),
> ongoing tasks, habit goals, and a richer quick-add.
> What changed per phase is in [`CHANGELOG.md`](CHANGELOG.md); decisions are in
> [`docs/DECISIONS.md`](docs/DECISIONS.md), and later plans in [`docs/WORKPLAN.md`](docs/WORKPLAN.md).

What works today:

- **Today view** (the landing page): a timeline with a "now" line, an ordered plan, what's up
  next, due/overdue tasks, progress, and browsing other days.
- **Drag and drop** with mouse, touch (long-press) or keyboard (grip + arrow keys):
  - reorder tasks and projects;
  - drop a task on a project, Inbox or Today in the sidebar (or the Today/Inbox tabs on a phone);
  - drop tasks onto the timeline to schedule them, drag blocks to move them in time, drag a
    block's bottom edge to change its duration, and drag a block back into the plan to unschedule it.
- **Your calendar**: connect any CalDAV calendar (Nextcloud, Fastmail, iCloud, Radicale, …;
  read-only). Events appear on the day's timeline and in a week **agenda**, and busy time is
  subtracted from the day's free time. Recurring events, exceptions and time zones are handled.
  Put an event (or every instance of a recurring one) into a project and attach todos to it.
  Optionally write your tasks with a time to one of your calendars as busy time.
- **Tasks from GitHub, GitLab and Gitea/Forgejo**: connect accounts with an access token; issues
  and pull/merge requests assigned to you, review requests and mentions arrive as tasks (rules
  choose the project) and complete themselves once closed or merged. Read-only.
- **Sharing with other Streamline servers**: someone with their own Streamline can join one of
  your group projects with an invite link; their server keeps a copy and sends their changes to
  yours. Personal data stays on each server.
- **Long-term goals**: milestones, links to projects and tasks, progress that fills up as work
  gets done (or set by hand), and a weekly/monthly review.
- **Habit goals**: things to keep up, like "eat less junk", from a start date and for good or
  until a date. Each day you answer yes, partly or no (with a note); the Goals page shows the
  week, the streak and the share of good days, and the weekly review asks for your verdict.
- **Weekly review**: the week in numbers, loose ends to plan or drop, waiting items, projects
  without a next step, a note per goal, then a look at the coming week.
- **Week view**: a planner with a column per day and each day's load, or the week on an hour
  grid with events, timed tasks and time blocks; drag tasks between days and times.
- **Search and keyboard shortcuts**: Ctrl+K (⌘K) or "/" finds tasks, projects, routines and
  pages and acts on a task; single-key shortcuts for the common actions (? lists them).
- **Summary, journal and tracking**: a day summary (planned vs. done, routines, events, focus),
  a private journal with prompts, mood/weight/custom metrics with reminders, trend charts,
  CSV import/export and a full JSON export.
- **Time blocks and suggestions**: day templates with themed blocks (deep work, admin…) per
  weekday; overlap warnings with quick fixes; an explainable "suggest times" in the planner.
- **Namedays and birthdays**: a shared nameday calendar (the official Estonian list is downloaded
  on first start; admins can upload another); everyone picks their own people by searching names,
  and gets tasks like "buy a present" → "wish a happy birthday" with lead times.
- **Places**: give tasks a place (Home, Cottage, Town…); pick where you are, or let GPS detect it
  (HTTPS), and lists show what can be done there.
- **Activity log**: every day records what happened (done, started, added, missed, focus time,
  planning) in time order, for reflecting; copy it as text.
- **Projects, subprojects (any depth) and inbox** (a task can also be listed in several projects): quick-add, complete with undo, archive.
- **Quick-add**: type a task's steps while adding it, or add it as already done, as waiting for
  results, or as ongoing. **Process the inbox** one item at a time with one key per decision.
- **Ongoing tasks**: things you keep at over a period (reading a book): shown every day of
  their period, ticked as "worked on it today", counting the days, finished by hand.
- **Built-in help** at `/help` (sidebar, or Settings on a phone).
- **Routines and repeating tasks**: on a schedule (any iCalendar rule: weekdays, monthly on the
  last day, …), at a fixed time (put on the timeline), or N times a week/month. Each occurrence is
  a normal task; skip one, edit just one, or change the routine from a date on with history kept;
  streaks per routine.
- **Household groups**: share projects, tasks and routines with a group; anyone can complete
  shared chores (who did is shown), changes appear live for everyone, and day plans, focus time and
  logs stay personal.
- **Prerequisites and multi-step chores**: tasks can wait for other tasks (and an extra wait,
  e.g. while the washing machine runs); multi-step chores like laundry (wash → dry → fold) with
  variants per kind of load; only the current step shows up as ready; routines can start them.
- **Views**: All tasks, the Inbox and every project as a list, a board (columns by status,
  project, difficulty or task type) or an urgent/important matrix; drag cards between
  columns/quadrants to change them. Each place remembers its view on all your devices.
- **Focus timer**: a distraction-free view of one task with a Pomodoro timer (25/5/15 by default)
  that runs on the server, so it's the same on every device; a mini timer on other pages, a chime
  and buzz between intervals, and focus time added to the task's time spent.
- **Daily planning ritual**: a five-step planner (review → look ahead → pick → arrange →
  confirm) for tomorrow in the evening or today in the morning, with planned vs. free time,
  an overbooking warning, a reminder at your planning time and a banner until the day is planned.
- **Pull tasks into a day** from the ready stack, give them a time and duration, or move them to
  another day.
- **Task details**: notes, due date, estimate, difficulty, importance and urgency, and task type.
- **Checklists and default tasks**: a task or routine can hold its steps (sort, wash, hang,
  fold) with progress shown everywhere; save a task as a *default task* and quick-add suggests
  it as you type, checklist included.
- **Waiting for results**: park a task whose work is handed off (a training run, a reply) with a
  check-back time; it asks for nothing meanwhile, then reminds you and returns to Up next.
- **Realistic estimates**: learned from the focus time on finished tasks; the task sheet
  suggests a better estimate when yours are usually off, and planning can count with them.
- **Morning digest**: one message each morning (push, ntfy or in the app) with the plan, fixed
  times, what's due and what to check on.
- **Print the day**: an A4 sheet with tick boxes and a QR code; scan it later to tick off what
  was done on paper.
- **Project ideas and marks**: keep not-yet-started projects as quiet *ideas* with a
  description; every project gets its own colour and shape.
- **Health check-in**: once a day, great / OK / unwell / sick (what) / injured (where), shown on
  the calendar and in trends.
- **Guided tour** on first sign-in (replay it from Help or Settings).
- **End-of-day behaviour by task type**: *Carry on* tasks roll over to the next day (with a
  "carried N days" badge), *Expires* tasks are marked missed. The day ends at a time you choose
  (default 04:00).
- **Live sync**: changes appear instantly on all your devices.
- **Installable app with notifications** (over HTTPS): add it to your phone's home screen, get
  reminders and alerts even when it's closed, and read your day offline. Daily automatic backups.
- **Multiple users** (admin-created), per-device **API tokens**, and a versioned JSON API.

## Quick start (Docker)

Each [release](https://github.com/Laurits7/Streamline/releases) has a `docker-compose.yml` that
uses the prebuilt image of that version (amd64 and arm64). Put it in an empty directory and run:

```sh
docker compose up -d
```

Or build from source:

```sh
git clone <this repo> streamline && cd streamline
docker compose up -d --build
```

Open `http://<server-ip>:3000`. On the first visit you create the admin account; add more
people under **Settings → Users**.

Everything is stored in `./data` (a single SQLite database plus its WAL files).

### Configuration

Set these in `docker-compose.yml` under `environment:`.

| Variable | Default | Meaning |
|---|---|---|
| `PORT` | `3000` | Host port in `docker-compose.yml` (the container always listens on 3000) |
| `INITIAL_ADMIN_USER` / `INITIAL_ADMIN_PASSWORD` | – | Create the first admin on startup instead of via the web UI |
| `TRUST_PROXY` | `false` | Set to `true` behind a reverse proxy, so `X-Forwarded-Proto/Host` are trusted (secure cookies over HTTPS) |
| `COOKIE_SECURE` | `auto` | `auto`, `true` or `false`. `auto` marks cookies `Secure` only when the proxy reports HTTPS |
| `SESSION_DAYS` | `90` | How long a login lasts |
| `SECRET_KEY` | – | Encrypts stored passwords (e.g. your calendar's). If unset, a random key is created in `data/secret.key`; keep it with your backups |
| `NAMEDAYS_URL` | stat.ee list | Where to download the nameday calendar on first start; `off` to never download (admins can upload a list instead) |
| `PUSH_CONTACT` | `mailto:streamline@example.org` | Contact sent to push services with notifications (use your e-mail as `mailto:you@…`) |
| `PUBLIC_URL` | – | The address Streamline is reached at (e.g. `https://todo.example.org`); used for links in ntfy notifications |
| `BACKUP_HOURS` / `BACKUP_KEEP` | `24` / `7` | Automatic backups into `data/backups/` (0 hours = off) and how many to keep |
| `LOG_LEVEL` | `info` | e.g. `debug`, `info,sqlx=warn` |
| `DATA_DIR` | `/data` | Where the database lives (inside the container) |

### HTTPS / reverse proxy

Plain HTTP on your LAN works for everything except installing it as an app, push notifications
and GPS places, which browsers only allow over HTTPS. Put Streamline behind a proxy that
provides HTTPS and set `TRUST_PROXY: "true"` (and optionally `PUBLIC_URL`). Live sync uses
server-sent events, so the proxy must not buffer responses.

**Caddy** (automatic certificates):

```
todo.example.org {
    reverse_proxy streamline:3000 {
        flush_interval -1   # deliver live-sync events immediately
    }
}
```

A complete, tested setup (Streamline + Caddy, public or internal certificates) is in
[`docs/examples/caddy`](docs/examples/caddy).

**Traefik** (labels on the `streamline` service; assumes an existing Traefik with a
`websecure` entrypoint and a certificate resolver named `le`):

```yaml
    labels:
      - traefik.enable=true
      - traefik.http.routers.streamline.rule=Host(`todo.example.org`)
      - traefik.http.routers.streamline.entrypoints=websecure
      - traefik.http.routers.streamline.tls.certresolver=le
      - traefik.http.services.streamline.loadbalancer.server.port=3000
```

Traefik streams server-sent events without extra settings.

**Tailscale** (private, nothing exposed to the internet): on the host, `tailscale serve --bg 3000`
gives `https://<machine>.<tailnet>.ts.net` with a valid certificate on every device in your
tailnet. Set `TRUST_PROXY: "true"`. This is the easiest way to get notifications on your phone.

### Install it on your phone, notifications

Over HTTPS, Streamline can be installed as an app and send notifications to your devices even
when it's closed:

- **iPhone/iPad (iOS 16.4+):** open it in Safari, tap **Share → Add to Home Screen**, open it from
  the Home Screen, then **Settings → Notifications → Turn on**.
- **Android / desktop Chrome, Edge:** use the browser's *Install app* menu (optional), then
  **Settings → Notifications → Turn on**.

Notifications go through the browser maker's push service (Apple, Google, Mozilla), so the server
needs outgoing internet access; it doesn't need to be reachable from the internet. Without HTTPS,
everything else works as before, and the optional [ntfy](https://ntfy.sh) channel (Settings →
Notifications) still delivers notifications. The push signing key is generated as
`data/vapid.key`; keep it with your backups, otherwise devices just have to turn notifications on
again. Offline, an installed app shows the data it last loaded (read-only).

### Backup and restore

Streamline copies its database to `data/backups/` every day and keeps the newest 7 (change with
`BACKUP_HOURS` / `BACKUP_KEEP`; `BACKUP_HOURS=0` turns it off). Admins can make one now and
download backups in **Settings → Backups**. Also copy the whole `data/` folder somewhere else from
time to time: besides the database it holds `secret.key` (needed to read stored calendar
passwords) and `vapid.key` (push notifications).

To restore: stop the container, copy the backup to `data/streamline.db`, delete
`data/streamline.db-wal` and `data/streamline.db-shm` if present, and start it again.

### Updating

Make a backup first (**Settings → Backups → Back up now**, or copy `data/`), then:

```sh
git pull && docker compose up -d --build
```

With the prebuilt image: `docker compose pull && docker compose up -d`. Pin a version with
`image: ghcr.io/laurits7/streamline:1.4.0` instead of `latest` if you prefer to update on your own
schedule. Database migrations run automatically on startup; going back to an older version after a
migration means restoring the backup.

### Sharing a project with someone on another Streamline server

People don't need an account on your server to work on a project with you: if they run their own
Streamline, share the project from yours. Share a top-level project with a group you own, then
open the project's menu → **People on other servers** → **Make an invite link**, and send the
link to that one person. They paste it in their Streamline under **Settings → Projects from
other servers**. Their server keeps a copy of the project (subprojects, tasks, routines) and
sends their changes to yours, which checks them as it would for anyone in the group. Day plans,
journals, tracking and calendars stay on each person's own server.

- Your server must be reachable from theirs (over HTTPS when it's on the internet; see above).
  Theirs never needs to be reachable: it only connects out to yours.
- While your server is down, their copy is read-only and says so.
- Invite links work once, for 7 days. Removing someone (same menu) or taking the project out
  of the group ends their copy. They can leave from their Settings.
- To try it on one machine: `docker compose -f docker/docker-compose.two-servers.yml up --build`
  starts two servers at http://localhost:3001 and http://localhost:3002.

### Raspberry Pi (arm64)

Two options:

- **Prebuilt image** (published by CI from `main` for amd64 and arm64): in `docker-compose.yml`,
  remove the `build:` section and set `image: ghcr.io/laurits7/streamline:latest`.
- **Build on the Pi**: works as-is. On a model with 1–2 GB of RAM, uncomment the `args:` in
  `docker-compose.yml` (`LTO: thin`, `CODEGEN_UNITS: "16"`) so compiling fits in memory.

## Desktop app (Linux)

Streamline in its own window, for Linux desktops (x86_64). It either **connects to your
Streamline server** (the address you open in the browser; it signs in once and keeps a
per-computer key in the system keyring) and keeps working offline, sending changes when the
server is back, or runs **on this computer only**, with the server inside the app and your
data in `~/.local/share/io.github.laurits7.streamline/server`. It stays in the tray when
closed, can start at login, and can mute the speakers and show the focus timer on every
screen. See *Help → Desktop app* for details, including moving local data to a server later.

- **Arch Linux** (AUR): `streamline-desktop-bin` (the release build) or `streamline-desktop`
  (built from source), e.g. `yay -S streamline-desktop-bin`.
- **Debian, Ubuntu**: download `streamline-desktop_<version>_amd64.deb` from the
  [release](https://github.com/Laurits7/Streamline/releases) and `sudo apt install ./streamline-desktop_<version>_amd64.deb`.
- **Other distributions**: the release's `streamline-desktop-<version>-x86_64-linux.tar.gz`
  holds `usr/` to unpack into `/` or `~/.local`. It needs WebKitGTK 4.1, GTK 3 and
  libayatana-appindicator; muting needs `pactl` (PipeWire or PulseAudio).

The corner timer is see-through only with a compositing window manager; window placement and
always-on-top are reliable on X11 and may be ignored by Wayland compositors.

## Security

- **First run:** the first person to open a fresh install creates the admin account. If the server
  is reachable by others before you've done that, set `INITIAL_ADMIN_USER` and
  `INITIAL_ADMIN_PASSWORD` instead.
- Passwords are hashed with Argon2; sign-in attempts are rate-limited; sessions and API tokens
  can be revoked (changing your password revokes your API tokens). Calendar passwords are stored
  encrypted (`data/secret.key`).
- Responses carry a strict Content-Security-Policy and no-framing headers; state-changing requests
  are checked against the page's origin.
- Streamline makes outbound requests only to addresses you give it (calendar, ntfy) and to push
  services. Local network addresses are allowed (for a home calendar server); cloud-metadata and
  link-local addresses are refused.
- Projects shared with another server: the invite link is the credential (single use, 7 days);
  the other server then holds a per-link key that only reaches that project, and can be cut off
  at any time. The name and server shown for someone who joined are what their server says.
- Journal, tracking, day plans and calendar events are visible only to their owner, even inside a
  shared group. A shared item can be taken private or deleted by any member of its group.
- Keep `data/` private: it holds the database, backups and keys. Use HTTPS when it's reachable
  from outside your home network.

## Resource use

Measured at the end of Phase 2 (x86-64, `scripts/measure.sh`): Docker image 7.5 MB, about
4 MB of memory while idle, and 48 KB gzipped for the first load of the web app (Help loads
separately, 4 KB).

## API

All endpoints are under `/api/v1` and use JSON. The full, always-current description is the
OpenAPI document at `/api/v1/openapi.json` (generated from the code), and you can read it at
`/api/docs` (the viewer loads from a CDN). Authenticate with the session cookie (web app)
or `Authorization: Bearer <token>`, using a token created in **Settings → API tokens**.

| Endpoint | Purpose |
|---|---|
| `GET /sync?since=<rev>` | All your data (`since=0`) or everything changed after `rev`, including deletions (`deleted_at` set) |
| `GET /events` | Server-sent events: `change` (`{rev, kind, data}`), `resync`, and `hello` on each connect |
| `GET /today` | Your current day (`YYYY-MM-DD`), taking timezone and day end into account |
| `POST/PATCH/DELETE /projects[/{id}]` | Projects |
| `GET/POST/PATCH/DELETE /tasks[/{id}]` | Tasks. `PATCH {"status":"done"}` completes a task |
| `GET /days/{date}` | A day's plan entries plus their tasks and anything due |
| `POST /days/{date}/entries` · `PATCH/DELETE /day-entries/{id}` | Plan, move, schedule and unplan tasks |
| `GET/PATCH /me`, `POST /me/password`, `GET/POST/DELETE /tokens` | Account |
| `GET/POST/PATCH/DELETE /users` | User management (admin only) |

Clients may choose ULID ids when creating records, so they can work optimistically or offline.
Retrying a create with the same id is safe. TypeScript types for every response are in
[`web/src/lib/api/types`](web/src/lib/api/types), generated from the Rust structs.

## Development

You need Rust (1.88 or newer) and Node 22 or newer.

```sh
make dev-server   # Rust API on :3000 (data in ./data)
make dev-web      # Vite on :5173 with hot reload, proxying /api to :3000
make test         # Rust tests + Svelte type check
make check        # fmt + clippy + svelte-check
make types        # regenerate TypeScript types from Rust
```

Layout:

```
crates/domain   pure logic (day boundaries, ordering keys, end-of-day rules), unit-tested
crates/server   Axum HTTP API, auth, SSE, background jobs, embedded web app
crates/desktop  Linux desktop app (Tauri 2; its own Cargo workspace)
migrations      SQLite schema
web             Svelte 5 + Vite single-page app
packaging       .desktop file, .deb and tarball scripts, AUR PKGBUILDs
docs            spec, work plan, decisions
```

The desktop app needs `webkit2gtk-4.1` and `libayatana-appindicator` (Debian/Ubuntu:
`libwebkit2gtk-4.1-dev libayatana-appindicator3-dev`). `make desktop` builds it into
`crates/desktop/target/release/streamline-desktop`; `cargo test --manifest-path
crates/desktop/Cargo.toml` runs its tests (offline sync against a real in-process server).

### Releasing

1. `scripts/bump-version.sh 1.4.0` (server, desktop app, web app and lock files), move the
   changelog's *Unreleased* heading to the version, commit, tag `v1.4.0` and push the tag.
2. CI publishes the image; the Release workflow then creates the GitHub release with the
   compose file and adds the desktop tarball and `.deb`.
3. **AUR** (needs your AUR account and its SSH key): `packaging/aur/update.sh 1.4.0` sets
   `pkgver`, the checksums of the released files and `.SRCINFO` for both packages. Commit
   that, then for each package:

   ```sh
   git clone ssh://aur@aur.archlinux.org/streamline-desktop-bin.git /tmp/aur-bin   # first time
   cp packaging/aur/streamline-desktop-bin/{PKGBUILD,.SRCINFO} /tmp/aur-bin/
   cd /tmp/aur-bin && git add PKGBUILD .SRCINFO && git commit -m "1.4.0" && git push
   ```

   and the same with `streamline-desktop`. The first push creates the package on the AUR.
   Check a package locally first with `makepkg -si` in its folder and `namcap` on the result.

## License

GPL-3.0-or-later
