# Streamline

A self-hosted todo list and day planner for a household. One small container, one data
folder, and it works from any phone, tablet or computer on your network.

> **Status: Phases 1–4 done (foundation, day plan, planning ritual, views and focus, routines, prerequisites and multi-step chores, household groups).** See [`docs/WORKPLAN.md`](docs/WORKPLAN.md)
> for what's built and what's next, and [`docs/SPEC.md`](docs/SPEC.md) for the full vision.

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
- **Long-term goals**: milestones, links to projects and tasks, progress that fills up as work
  gets done (or set by hand), and a weekly/monthly review.
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
- **End-of-day behaviour by task type**: *Carry on* tasks roll over to the next day (with a
  "carried N days" badge), *Expires* tasks are marked missed. The day ends at a time you choose
  (default 04:00).
- **Live sync**: changes appear instantly on all your devices.
- **Multiple users** (admin-created), per-device **API tokens**, and a versioned JSON API.

## Quick start (Docker)

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
| `LOG_LEVEL` | `info` | e.g. `debug`, `info,sqlx=warn` |
| `DATA_DIR` | `/data` | Where the database lives (inside the container) |

### HTTPS / reverse proxy

Plain HTTP on your LAN works. To use HTTPS (needed later for installing it as an app and for
push notifications), put it behind a proxy and set `TRUST_PROXY: "true"`. Caddy example:

```
todo.example.home {
    reverse_proxy streamline:3000 {
        flush_interval -1   # deliver live-sync events immediately
    }
}
```

A complete, tested setup (Streamline + Caddy, automatic or internal certificates) is in
[`docs/examples/caddy`](docs/examples/caddy).

Tailscale: `tailscale serve --bg 3000` on the host also works (set `TRUST_PROXY: "true"`).

### Backup and restore

Stop the container and copy the `data/` folder (it includes `secret.key`, needed to read stored
calendar passwords; without it you just enter them again). To back up while it's running, use SQLite's
online backup:

```sh
sqlite3 data/streamline.db ".backup 'streamline-backup.db'"
```

To restore, stop the container, put the backup in place as `data/streamline.db` (remove any
`-wal`/`-shm` files), and start it again.

### Updating

```sh
git pull && docker compose up -d --build
```

Database migrations run automatically on startup.

### Raspberry Pi (arm64)

Two options:

- **Prebuilt image** (published by CI from `main` for amd64 and arm64): in `docker-compose.yml`,
  remove the `build:` section and set `image: ghcr.io/laurits7/streamline:latest`.
- **Build on the Pi**: works as-is. On a model with 1–2 GB of RAM, uncomment the `args:` in
  `docker-compose.yml` (`LTO: thin`, `CODEGEN_UNITS: "16"`) so compiling fits in memory.

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
migrations      SQLite schema
web             Svelte 5 + Vite single-page app
docs            spec, work plan, decisions
```

## License

GPL-3.0-or-later
