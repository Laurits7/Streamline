# Streamline: Work Plan

> Companion to [`SPEC.md`](SPEC.md). Section numbers like §6.2c refer to the spec.
> Status (2026-10-06): Part A and Part C **confirmed** (see `DECISIONS.md`). **Phases 1, 2, 2a, 2b
> and 3 are complete**, plus the owner's follow-ups (orange unplanned banner, Done → To do on the board,
> tasks in several projects, daily activity log; D-48), routine versions (D-49) and 2b.6a Places (D-50).
> Phases 3b (prerequisites, multi-step chores) and 4 (groups) are complete too. Next: Phase 5
> (calendar / CalDAV) — 3b.6a Occasions waits for Q-24 (nameday calendar source).
> Legend: `[x]` done · `[~]` partly done (the remaining work is noted) · `[ ]` not started.

## MVP status (2026-10-06)

Runs with `docker compose up -d`: first-run admin setup, users/API tokens, projects, inbox, tasks
with all §6.6 attributes, Today view (scheduled/plan/due/done, now line, up next, day
navigation), pull-in sheet, drag-to-reorder lists, carry-on/expires rollover at a configurable
day end, live multi-device sync. Remaining Phase 1/2 work is marked `[~]` below.

## How to use this plan

- The work is split into **phases** that follow §7. Each phase is split into **chunks**.
  A chunk is sized for one focused Claude Code session and ends in a commit (or PR) that
  builds, passes tests, and leaves the app runnable.
- A phase ends with a **phase gate** (Part D): resource measurements, a manual smoke test,
  and updates to `README.md` and `docs/DECISIONS.md`.
- Tick the checkboxes as chunks land. If a chunk turns out bigger than one session, split it
  here first, then do the work.
- When a chunk needs an open question settled, it says so (`needs Q-x`, see Part C).
  Don't start that chunk until the owner has confirmed the default or picked another option.

---

## Part A. Architecture baseline (proposed, confirm before Phase 1)

### A.1 Repository layout

```
Streamline/
├── Cargo.toml                # Cargo workspace
├── crates/
│   ├── domain/               # Pure logic, no IO: recurrence, rollover, dependency graph,
│   │                         # conflict detection, planner, task-type rules. Most tests live here.
│   ├── server/               # Axum binary: config, HTTP API, auth, SSE, jobs, DB access,
│   │                         # embeds the built frontend
│   ├── caldav/               # (Phase 5) CalDAV client + iCal parsing behind a provider trait
│   └── integrations/         # (Phase 7) Mealie and later providers
├── migrations/               # sqlx migrations (SQLite)
├── web/                      # SvelteKit app (adapter-static, SPA mode)
│   └── src/lib/api/types/    # TS types generated from Rust (ts-rs); committed, checked in CI
├── docker/
│   ├── Dockerfile            # multi-stage: node build → rust musl build → scratch/distroless
│   └── docker-compose.yml    # app (+ optional radicale profile for dev)
├── scripts/                  # measure.sh, gen-types.sh, dev helpers
├── docs/
│   ├── SPEC.md  WORKPLAN.md  DECISIONS.md  API.md (links to generated OpenAPI)
└── justfile                  # dev, test, build, measure, gen-types
```

Why split out `domain`: the spec's risky logic (§8: recurrence, rollover, dependencies,
conflicts, planner) is pure and deterministic. Keeping it free of DB and HTTP code makes it fast
to test and easy to reuse later, e.g. in a Tauri client.

### A.2 Backend conventions

| Topic | Proposal |
|---|---|
| Framework | Axum + Tokio + tower-http (compression, tracing, timeouts). |
| DB | SQLite via sqlx, WAL mode, `foreign_keys=ON`, a single writer pool plus a read pool. Migrations embedded with `sqlx::migrate!`. |
| IDs | ULID, stored as `TEXT` (sorts by time, safe to create offline). |
| Timestamps | Instants stored as UTC RFC 3339 `TEXT`. Local dates as `YYYY-MM-DD`, local times as `HH:MM`, plus the user's IANA timezone. |
| Sync fields | Every entity has `created_at`, `updated_at`, `deleted_at` (tombstone) and `rev`. `rev` is a global, monotonically increasing change sequence. `GET /api/v1/changes?since=<rev>` returns everything changed since then. SSE events carry the same `rev` (used as SSE `id`). |
| Ordering | Fractional-index string keys (`position TEXT`), so reordering touches one row and stays sync-friendly. |
| API | `/api/v1/...` JSON. Errors use RFC 7807 problem+json. OpenAPI generated with `utoipa` and served at `/api/v1/openapi.json`, with a docs UI only in dev builds to keep the binary small. |
| Types to TS | `ts-rs` on the API DTOs. `just gen-types` writes them to `web/src/lib/api/types`, and CI fails if they are stale. |
| Auth | Argon2id passwords. Sessions in a DB table, with the cookie holding a random token (stored hashed). Cookie is `HttpOnly` and `SameSite=Lax`, and `Secure` only when the request is HTTPS (directly or via a trusted `X-Forwarded-Proto`). Per-device API tokens sent as `Authorization: Bearer`, stored hashed, revocable. Requests that change data and use cookie auth must pass an Origin/Host check. Login is rate-limited. |
| Proxy-awareness | No hard-coded scheme or host. Absolute URLs (QR links, push) come from `PUBLIC_URL` if set, else from forwarded headers when `TRUST_PROXY=true`, else from the request itself. |
| Realtime | A `tokio::sync::broadcast` bus feeds the SSE endpoint `GET /api/v1/events`. Events are filtered per subscriber by a single **visibility function** (A.4). Supports `Last-Event-ID` resume (the server replays from `rev` or tells the client to refetch) and sends a heartbeat every 25 s. |
| Jobs | One in-process scheduler (Tokio interval, 30–60 s tick) for: day rollover, occurrence materialization, wait-time readiness, reminders, calendar sync. All jobs are idempotent. No external cron. |
| Config | Env vars (`PORT`, `DATA_DIR`, `SECRET_KEY`, `INITIAL_ADMIN_USER/PASSWORD`, `PUBLIC_URL`, `TRUST_PROXY`, `LOG_LEVEL`), plus a settings table for anything editable in the app. `SECRET_KEY` is generated on first boot into `data/` if not given. |
| Secrets at rest | CalDAV and integration credentials encrypted with a key derived from `SECRET_KEY` (XChaCha20-Poly1305). They are never returned by the API or written to logs. |
| Static assets | The built SPA is embedded with `rust-embed` and served with precompressed br/gzip variants and immutable cache headers for hashed files. Unknown non-API paths fall back to `index.html`. |
| Image | Frontend built with node:alpine. Rust built for `*-unknown-linux-musl` with `opt-level="z"`/`"s"`, LTO, strip, and the musl allocator swapped for mimalloc if RSS benefits. Final stage is `scratch` (or distroless static) with CA certs. Images built for amd64 and arm64 (Raspberry Pi). |

### A.3 Frontend conventions

- Svelte 5 + SvelteKit with `adapter-static` (SPA fallback) and TypeScript. No UI kit.
  Plain CSS with design tokens (custom properties), light and dark themes, a fluid type scale.
- Layout: bottom tab bar on phones (Today · Projects · Plan · More), a sidebar on tablet and
  desktop, sheets/drawers for editing (no full-page forms).
- **Client store**: a normalized entity cache keyed by ID. Every mutation goes through
  `mutate()`, which (1) applies the change locally right away, (2) sends it, and (3) on failure
  rolls back and shows a toast. SSE events are merged in by `rev`, so a device's own echoes do
  nothing.
- Small hand-picked dependencies only: a date helper, tree-shaken icons, and drag-and-drop
  (choose in 2.6). Charts are custom SVG components (§6.2b).
- Bundle budget: < 150 KB gzip for the initial route, enforced by a CI check (target in the
  spec is < 300 KB).

### A.4 Core data model decisions (the ones that are hard to change later)

1. **Everything actionable is a `task` row.** One-off tasks, recurring occurrences and workflow
   steps are all tasks. A recurring item is a **`series`** row (RRULE, mode, template fields)
   that *spawns* task rows with `series_id` and `occurrence_key` (date or window). A workflow
   run is a **`workflow_instance`** row whose steps are task rows. Because of this, completion,
   day-plan entries, dependencies, focus sessions, task types, groups and the external reference
   only need to work on one kind of thing. Occurrences are created **lazily within a horizon**
   (from the past rollover up to today + N days, plus on demand for a viewed date), never as an
   unbounded expansion.
2. **Completion and history**: `tasks.status` (`open|done|missed|skipped|wont_do`) plus
   `completed_at`/`completed_by`. An append-only `task_events` log records carried, missed,
   snoozed, skipped, completed and reopened events, each with the actor and source
   (`app|api|print|integration`). Stats and streaks are computed from this log.
3. **Task types are data.** A `task_types` table with config columns: `day_end_behavior`
   (`carry|expire|window|deadline`), `counts_for_streak`, `shows_overdue`, `shows_carry_count`,
   `window_overflow` (`miss|roll`). Built-in rows are seeded. Users may add their own in v1
   (needs Q-1). The rollover engine in `domain` reads only this config, with no special cases
   by type name.
4. **Ownership and visibility**: a task or project has `owner_user_id` *or* `owner_group_id`
   (DB CHECK: exactly one). A task also has a nullable `assignee_user_id`, unused until Q-4 is
   decided. A single `visible_to(user)` SQL fragment plus a Rust predicate is used by every
   query and by SSE filtering. Personal data (day records, metrics, focus sessions) is
   **always** filtered by user only, never by group.
5. **Day plan is a separate layer**: `day_plan_entries(user_id, date, task_id, position,
   start_time?, duration_min?, block_id?)`. Tasks know nothing about day plans.
6. **External reference from day one**: `tasks.ext_source`, `ext_id`, `ext_url`,
   `ext_payload JSON`, with a unique index on (`ext_source`, `ext_id`) for deduplication (needed
   by Mealie and GitHub later).
7. **"Logical date"**: one domain function maps (instant, user timezone, day-end time) to the
   local date it belongs to. Every "today" comes from this function. The server is the source of
   truth, and the client asks the server what "today" is.

---

## Part B. Phases and chunks

Legend: **Deliverables** = what exists afterwards · **Tests** = what must be covered ·
**Done when** = acceptance check.

### Phase 0: Planning (this document)

- [x] **0.1 Confirm plan.** Owner reviews Part A and Part C and answers or confirms the defaults.
      Record the answers in `docs/DECISIONS.md` (ADR-style, one short entry per decision).

---

### Phase 1: Foundation (§7.1, §3, §4, §4b, §6.1)

- [x] **1.1 Scaffold and toolchain** (Makefile instead of justfile, D-26; CI in `.github/workflows/ci.yml`)
  - Deliverables: Cargo workspace (`domain`, `server`), SvelteKit app in `web/`, `justfile`
    (`dev` runs Vite with a proxy to the backend, plus `test`, `lint`, `build`), rustfmt/clippy,
    prettier/eslint/svelte-check, `.editorconfig`, `.gitignore` additions (`data/`, `target/`,
    `node_modules/`, `web/build/`). Check and pin current stable versions of Axum, sqlx, Svelte
    and SvelteKit.
  - CI (GitHub Actions): fmt, clippy `-D warnings`, cargo test, web lint/check/test, build.
  - Done when: `just dev` serves a hello page via the Rust server, and CI is green.

- [x] **1.2 Server skeleton**
  - Deliverables: config loading (env + defaults + validation), tracing, `/healthz`, graceful
    shutdown, embedded SPA with fallback and precompressed assets, problem+json error type,
    request ID, compression, proxy-awareness helper (`PUBLIC_URL`/`TRUST_PROXY`).
  - Tests: config parsing; scheme/host resolution with and without forwarded headers.

- [x] **1.3 Database and conventions**
  - Deliverables: sqlx pool (WAL, pragmas), migration runner at startup, `rev` counter,
    a shared helper/trait for create/update/soft-delete (stamps timestamps and `rev`), ULID
    type. First migration: `users`, `sessions`, `api_tokens`, `settings`, `projects`, `tasks`
    (including the A.4 columns: owner, assignee, ext_*, attribute columns nullable for now),
    `task_types` (seed `carry_on`, `expires`), `task_events`.
  - Tests: migrations apply cleanly on an empty DB; soft-delete hides rows; `rev` increases
    monotonically under concurrent writes.
  - Note: add attribute columns now (estimate, difficulty, importance, urgency,
    actual_minutes) so Phase 2 doesn't need an awkward migration.

- [x] **1.4 Auth**
  - Deliverables: initial admin created from env on first boot; login/logout; session
    middleware; `me` endpoint; change password; admin-only user CRUD (no public sign-up);
    API token create/list/revoke; Origin check for cookie-based writes; login rate limit.
  - Tests: wrong password, expired or revoked session/token, CSRF origin rejection, cookie
    `Secure` flag over HTTP vs HTTPS (proxied), admin-only routes.
  - Done when: `curl` with a bearer token and the browser with a cookie both reach `/api/v1/me`.

- [x] **1.5 Projects and tasks API** (OpenAPI 3.1 via utoipa at `/api/v1/openapi.json`, viewer at `/api/docs`)
  - Deliverables: projects CRUD and archive; tasks CRUD, complete/reopen, move between
    projects, reorder (fractional index), inbox (`project_id IS NULL`), list filters;
    `GET /changes?since=`; utoipa OpenAPI; ts-rs type generation script.
  - Tests: integration tests via `tower::ServiceExt::oneshot` on an in-memory DB, covering
    CRUD, reorder, move, archive hiding, tombstones in the change feed, and ownership checks
    (a user cannot read another user's tasks).

- [x] **1.6 Realtime event stream**
  - Deliverables: event bus; `GET /api/v1/events` SSE with heartbeat and `Last-Event-ID`
    resume; events emitted from the shared helper for every entity change; visibility filter
    hook (user-only for now, groups added in Phase 4).
  - Tests: subscriber receives own-user events and not other users' events; resume after
    reconnect.

- [x] **1.7 Frontend shell** (Vitest store/order tests and a CI bundle budget)
  - Deliverables: routing, responsive layout (tab bar / sidebar), theme tokens + dark mode,
    login page, API client on top of generated types, normalized store with `mutate()`
    (optimistic + rollback), SSE subscription with reconnect, toast system, empty states.
  - Tests: vitest for the store (optimistic apply, rollback on error, SSE merge ignores echoes).
  - Done when: login works on a phone and on desktop, and the bundle stays within budget.

- [x] **1.8 Projects and tasks UI**
  - Deliverables: quick-add inbox (one input, Enter to add), project list (create, rename,
    archive), task list (complete with animation, inline edit, detail sheet for notes and due
    date), move-to-project menu, reorder via up/down menu (drag arrives in 2.6); settings page
    (change password, API tokens, admin user management).
  - Done when: two browser windows stay in sync live; actions feel instant with network
    throttling set to "Slow 3G".

- [x] **1.9 Packaging and phase gate** (multi-arch CI image → GHCR; Caddy example in `docs/examples/caddy`, tested over HTTPS 2026-10-06; arm64 build itself only verified once CI runs it)
  - Deliverables: multi-stage Dockerfile (amd64 and arm64), `docker-compose.yml` with
    `./data` volume and `PORT`, `scripts/measure.sh` (image size, idle RSS via `docker stats`
    after 60 s, initial gzip/brotli payload), README quick start, `docs/DECISIONS.md`
    started, `docs/API.md`.
  - Done when: `docker compose up -d` on a clean machine works over plain HTTP and behind a
    Caddy HTTPS proxy (sample Caddyfile in `docs/`). Phase gate passes (Part D).

---

### Phase 2: Day plan (§6.2, §6.2c, §6.6, basic §6.12)

- [x] **2.1 Time model and user preferences**  *(needs Q-2)* (timezone, day end, locale, week start; `day_bounds`/`week_bounds` with DST tests)
  - Deliverables: user prefs (timezone, day-end time, week start, locale); `domain::time`
    with `logical_date()`, day bounds, `HH:MM` handling.
  - Tests: DST transitions (Europe/Tallinn spring/autumn), a day end of 04:00 around midnight,
    timezone change mid-use. Check these carefully, since everything later depends on them.

- [x] **2.2 Task attributes**  *(needs Q-18, Q-22)*
  - Deliverables: API and validation for estimate, difficulty, importance, urgency,
    actual_minutes, due date; UI quick-setters (chip rows, not dropdowns); badges in lists.

- [x] **2.3 Task-type engine and day rollover**  *(needs Q-1)*
  - Deliverables: `domain::rollover`, a pure function from (task, type config, day outcome)
    to the resulting state and events; a rollover job that runs per user once their day end
    passes (idempotent via `last_rollover_date` per user, and catches up over several days
    if the server was down); carry counter; per-item overrides API: skip today, snooze to
    date, won't do, all logged in `task_events`. Types `carry_on` and `expires` are active.
  - Tests: carry vs expire at the boundary; catching up after 3 days of downtime; overrides;
    idempotency (running twice gives the same result).

- [x] **2.4 Day plan API**
  - Deliverables: `day_plan_entries` migration; pull into day, remove, reorder, set/clear
    time slot and duration; **ready stack** endpoint (open, not planned elsewhere, not blocked
    once Phase 3b lands); **`GET /api/v1/days/{date}`** aggregate (entries, due tasks,
    carried-over tasks, and later routines, events and blocks) so the Today view needs only
    one request.
  - Tests: one entry per task per day; carried tasks move to the next day according to type;
    the aggregate respects visibility.

- [x] **2.5 Today view (landing page)**
  - Deliverables: Today as the default route; sections for Now/Next, scheduled timeline
    (with a now marker), ordered unscheduled list, due/overdue, carried over (with "×3 days"
    badges); placeholders for events and routines; a "pull from ready stack" sheet; date
    navigation (yesterday/tomorrow).
  - Done when: opening the app on a phone shows the day in one screen within ~1 s on LAN.

- [x] **2.6 Basic drag and drop** (own pointer-based layer, D-36: reorder, drop onto projects/Inbox/Today, timeline scheduling/move/resize; tested in headless Chrome with mouse and emulated touch, real-device check still advisable)
  - Deliverables: choose a DnD approach (evaluate `svelte-dnd-action` against a small custom
    pointer-events action; pick on bundle size, touch quality and keyboard support, and log the
    choice in DECISIONS.md). Reorder within a list, drag from the ready stack into the day,
    drag onto a time slot. Long-press to start on touch, scrolling must not trigger a drag,
    keyboard pickup/move/drop, and a menu alternative for every drag.
  - Done when: it works on a real phone (iOS Safari + Android Chrome) and with the keyboard
    only.

- [x] **2.7 Phase gate** (2026-10-06: tests green, measurements logged, HTTP + HTTPS browser smoke tests, no secrets in trace logs)

---

### Phase 2a: Planning ritual, basic (§6.2d)

- [x] **2a.1 Plan status and preferences**  *(needs Q-8, Q-9)* (`day_plans` draft/planned, plan mode and times, available-day window; free time = window minus scheduled, for today only what's left)
  - Deliverables: `day_plan_status(user, date, planned_at)`; preferences (evening / morning /
    both, planning time); API to mark a day planned or unplanned; free-time calculation
    (day window minus fixed items; calendar events join in Phase 5).
- [x] **2a.2 Planning wizard** (`/plan/<date>`, 5 steps, resumes from the server-side draft step, lazy-loaded)
  - Deliverables: a 5-step full-screen flow (Review → Look ahead → Pick → Arrange →
    Confirm), each step one screen with a sticky "Next"; a running total of planned time vs.
    free time with a non-blocking overbooking warning; it resumes where the user left off if
    interrupted. The Review step uses the outcomes logged by the rollover engine.
  - Done when: a plan for a typical day can be made in under 2 minutes on a phone.
- [x] **2a.3 Reminders (in-app) and fallback**  *(needs Q-5)* (`notify` module, reminder job once per kind and day, toast + dismissible banner)
  - Deliverables: notification abstraction (`Notifier` trait: in-app via SSE now, with Web
    Push and ntfy added in Phase 6); a reminder job at the planning time; an "Unplanned: plan
    your day" banner in the Today view; the fallback Today view needs no plan.
- [x] **2a.4 Phase gate** (2026-10-06: tests green, measurements logged, browser run of the full ritual on desktop and phone)

---

### Phase 2b: Views and focus (§6.7, §6.9, §6.12)

- [x] **2b.1 View framework** (list/board/matrix on All tasks, Inbox and projects; per-scope prefs in `users.prefs`): view switcher (list / kanban / matrix), filters stored per
      view in server-side user prefs (so they follow the user across devices).
- [x] **2b.2 Kanban** (in progress = `started_at`, D-44; also by difficulty): columns by status (open / in progress / done), or by project or task
      type; drag between columns changes the matching field; adds an `in_progress` status.
- [x] **2b.3 Eisenhower matrix**  *(needs Q-22)*: four quadrants; drag sets importance and
      urgency; blocked tasks are dimmed (once 3b lands).
- [x] **2b.4 DnD everywhere and accessibility pass** (board/matrix targets, horizontal auto-scroll, aria-live announcements for drops, menu/sheet alternatives): drag tasks onto projects in the
      sidebar; one shared DnD layer for all views; screen reader announcements; menu
      alternatives everywhere.
- [x] **2b.5 Focus sessions backend**  *(needs Q-20)*
  - Deliverables: `focus_sessions` table; a server-side timer state machine per user
    (`idle → work → break → long_break`, pause/resume/skip), stored as start time + accumulated
    pause, so any device can reconstruct the countdown from the server state. Work sessions
    add to `tasks.actual_minutes`. SSE updates.
  - Tests: state transitions, pause accounting, long break after N sessions, recovery after a
    server restart.
- [x] **2b.6 Focus view UI**: distraction-free page (title, notes, step context), large timer,
      start/pause/skip, sound + `navigator.vibrate` cue, a mini-timer pill visible on all
      pages, and done / snooze / next-planned-task actions.
- [x] **2b.6a Places** (SPEC §6.16, D-45; done 2026-10-06, D-50): `places` per user/household (name, optional
      coordinates + radius); optional `place_id` on tasks with a per-project default for new
      tasks; place chip on rows; a "where am I" filter on the ready stack, pull-in sheet and
      views, picked manually, or detected by GPS when enabled in Settings and permitted by the
      browser (secure contexts only; falls back to manual).
- [x] **2b.7 Phase gate** (2026-10-06: tests green, measurements logged, browser run of views and focus on desktop and phone)

---

### Phase 3: Recurrence and routines (§6.3, remaining §6.2c types)

- [x] **3.1 Recurrence spike and module** (rrule 0.14: all edge cases pass, nothing hand-rolled, D-46)
  - Check how mature and well maintained the `rrule` crate is; check `chrono-tz` for
    timezones. Report any gaps to the owner. Then build `domain::recurrence`: expand an RRULE
    (with DTSTART, TZID, EXDATE, UNTIL/COUNT) into local dates/times within a range.
  - Tests: daily/weekly/monthly/BYDAY/BYSETPOS, last day of month, DST, EXDATE, COUNT across
    the horizon.
- [x] **3.2 Series and occurrence materialization**
  - Deliverables: `series` table (template fields, rrule, mode, task type, project/owner);
    materializer job that spawns task rows within the horizon (idempotent via a unique key
    on `series_id, occurrence_key`); "edit this occurrence" (detaches into an exception) vs
    "edit all future" (splits the series at that date: close the old one with UNTIL, start a
    new one); deleting future occurrences.
  - Tests: completing one occurrence does not affect others; editing all future keeps past
    history; the materializer is idempotent; changing the horizon.
- [x] **3.3 Anchored routines**: occurrences with a fixed time appear automatically in the
      Today view timeline; default type `expires`; missed occurrences are logged.
- [x] **3.4 Flexible routines and the window type**  *(needs Q-3)*
  - Deliverables: series with `times_per_window` and a window (week/month); spawns N task
    rows per window ("Laundry 1/2", "2/2") that can be done on any day; `window` task type
    activated (overflow: miss or roll into the next window, per config); UI shows "1 of 2
    this week".
  - Tests: window boundaries respect week start and day end; overflow behavior; completing
    early in the window.
- [x] **3.5 Deadline type, routines UI and streaks**
  - Deliverables: `deadline` type (carries on until the due date, then overdue); routines
    page; recurrence editor with presets (daily, weekdays, weekly on…, every N days, N times
    per week) plus a raw RRULE field for advanced use; streak calculation from `task_events`
    (only types with `counts_for_streak`).
- [x] **3.6 Phase gate** (2026-10-06: tests green, measurements logged, browser run of routines)

---

### Phase 3b: Prerequisites and workflows (§6.3b)

- [x] **3b.1 Dependency graph**  *(needs Q-6)*
  - Deliverables: `task_dependencies(task_id, depends_on_id)`; `domain::deps` with cycle
    detection (DFS over the affected subgraph) and readiness evaluation; a stored
    `tasks.blocked` flag kept up to date transactionally on every change, so the ready stack
    stays a cheap query; behavior when a prerequisite is skipped or deleted follows Q-6;
    an SSE "became ready" event.
  - Tests: reject direct and indirect cycles; diamond graphs; dependencies across projects
    and groups; completing the last prerequisite unblocks; reopening a prerequisite blocks
    again; blocked tasks never "miss" at rollover (§6.2c).
- [x] **3b.2 Blocked UI**: blocked badge with "waiting on …" links in project lists; hidden
      from the ready stack and the planner; dimmed in the matrix; add/remove prerequisite UI
      (task picker).
- [x] **3b.3 Workflow templates** (steps/variants as JSON on the template; linear chains, D-51)
  - Deliverables: `workflow_templates`, `workflow_steps` (title, type, estimate,
    wait_minutes), `workflow_step_edges`, `workflow_variants` + per-variant step inclusion;
    CRUD API; `instantiate(template, variants[])` creates one chain of tasks per selected
    variant with dependencies wired, all in one transaction; seed an example "Laundry"
    template.
  - Tests: variant inclusion and skipped steps re-linking the chain (wash → fold when dry and
    iron are skipped); everything rolls back if instantiation fails.
- [x] **3b.4 Wait time**: `ready_at` on dependent tasks (prerequisite done + wait); the
      scheduler flips them to ready; "mark ready now" override; countdown shown in the UI.
- [x] **3b.5 Workflow UI** (JSON export/import of templates (Q-12) still open): template editor as an ordered step list with "skip in variant"
      toggles (no graph editor); "Start Laundry" sheet with variant multi-select; chain
      progress dots on each task.
- [x] **3b.6 Routine-attached workflows**  *(needs Q-7)* (runs start on their day; per-routine override of D-7 not yet): a series can reference a template;
      each occurrence spawns an instance; policy for a still-unfinished previous instance
      (Q-7).
- [ ] **3b.6a Occasions: birthdays and namedays** (SPEC §6.17)  *(needs Q-24)*
  - Deliverables: bundled nameday calendar (data file + licence check); `people` with name(s)
    and optional birthday; "names of interest" picker; occasion templates = workflow templates
    whose steps carry a **day offset** from the occasion (e.g. buy present −2 d → greet 0 d,
    with a dependency); a yearly series per person/occasion spawns them ahead of time; default
    task type per occasion kind, editable.
  - Tests: nameday lookup incl. leap years and names with several days; offsets across month
    and year boundaries; greet stays blocked until the present step is done; no duplicate
    spawns.
- [x] **3b.7 Phase gate** (2026-10-06; 3b.6a Occasions still waits for Q-24)

---

### Phase 4: Groups (§6.4)

- [x] **4.1 Groups and visibility**
  - Deliverables: `groups`, `group_members`; admin and group-owner management; build out the
    `visible_to` function (A.4.4) and route **every** list/detail/SSE path through it.
  - Tests: a visibility matrix (owner / group member / non-member / admin) for projects,
    tasks, series, workflow instances, day-plan entries and personal data. This test is
    required to pass before the phase can close.
- [x] **4.2 Group projects, tasks and chores**  *(needs Q-4)*
  - Deliverables: create a project or task for a group; group tasks show up in every member's
    ready stack; **any** member completes it, and `completed_by` is shown; recurring group
    chores use the same series system (one shared occurrence, not one per member); a member's
    day plan can include a group task, and if another member completes it, it shows as done
    for everyone.
- [x] **4.3 Live sync between members**: SSE fan-out to group audiences; membership changes
      update subscriptions; UI shows group badges and avatars on completions.
      E2E test: two users, one completes the garbage chore, and it disappears for the other
      within 1 s.
- [x] **4.4 Phase gate** (2026-10-06: visibility matrix + live-stream tests, two-browser run)

---

### Phase 5: Calendar read (§6.5)

- [ ] **5.1 CalDAV and iCal spike** (report to the owner before building on it)
  - Add a `radicale` service to a dev compose profile. Prototype discovery
    (`current-user-principal` → `calendar-home-set` → list calendars via PROPFIND) and a
    `calendar-query` REPORT with a time range, using `reqwest` + `quick-xml`. Compare iCal
    parsers (`icalendar` with its parser feature vs. `ical`) on VTIMEZONE, RECURRENCE-ID and
    all-day events. Expected result: the CalDAV transport is hand-rolled (small); parsing and
    RRULE use crates. Log this in DECISIONS.md.
- [ ] **5.2 Provider abstraction and settings**
  - Deliverables: `CalendarProvider` trait (`list_calendars`, `fetch_events(range)`, and
    unimplemented `put_event`/`delete_event` reserved for later write-back); the CalDAV
    implementation; encrypted credential storage; settings API + UI (URL, user, password,
    "test connection", calendar selection and colors). The password is write-only in the API.
- [ ] **5.3 Sync engine and expansion**
  - Deliverables: periodic and on-demand sync; incremental via ctag/etag (sync-token where
    supported); cache of raw VEVENTs + expanded instances for a rolling window; handles
    RRULE, EXDATE, RECURRENCE-ID overrides, all-day and floating times, TZID; sync status and
    last error shown in settings; on failure, keep the last known data and show a warning.
  - Tests: fixture `.ics` files (recurring with exceptions, all-day across DST, floating time,
    non-UTC TZID, cancelled instances); mocked HTTP for the PROPFIND/REPORT XML.
- [ ] **5.4 Events in the UI**: events in the Today timeline (fixed, read-only); free-time
      gaps between events; agenda/week view; the planning wizard's free-time number now
      subtracts events.
- [ ] **5.5 Phase gate**

---

### Phase 5a: Conflicts and block planning (§6.8, §6.11)

- [ ] **5a.1 Conflict detection**: `domain::conflicts`, a pure interval-overlap function
      returning conflict pairs (task↔event, task↔task, block↔event); included in the day
      aggregate; alert banner and inline markers; quick fixes (move to next free slot, shorten,
      unschedule); notification through the `Notifier`.
      Tests: touching vs. overlapping edges, all-day events (ignored or blocking: configurable
      per calendar), midnight and day-end crossings.
- [ ] **5a.2 Time blocks and day-plan templates**: `time_blocks`, `day_plan_templates`,
      template blocks, weekday assignment; applying a template to a date (idempotent,
      editable afterwards); block view UI; drag tasks into blocks and resize/move blocks
      (with a menu alternative).
- [ ] **5a.3 Auto-suggest planner**  *(needs Q-19)*
  - Deliverables: `domain::planner::suggest(blocks, events, ready_tasks, prefs) -> Plan`.
    Stable sort by a documented score (deadline proximity, urgency/importance, difficulty
    matched to block theme), greedy fit into free time inside blocks, never overlapping
    events, ties broken by ID so results are deterministic. Each placement includes
    machine-readable **reasons** that the UI shows ("Hard + due tomorrow → Deep work block").
    Integrated as an option in the wizard's Arrange step: accept all / accept some / reject.
  - Tests: golden tests (fixed inputs → exact plan); never places on an event; respects
    readiness and task types; overbooked input → leftovers listed with a reason.
- [ ] **5a.4 Phase gate**

---

### Phase 5b: Daily summary and tracking (§6.2b)

- [ ] **5b.1 Day records and reflection**  *(needs Q-16)*: lazily created `day_records`;
      free text + optional prompt layout (went well / didn't / tomorrow) with autosave;
      the evening flow chains reflection → plan tomorrow.
- [ ] **5b.2 Metrics**  *(needs Q-23)*: `metric_definitions` (built-in mood and weight, plus
      custom number/scale/yes-no with unit and reminder), `metric_entries` (several per day);
      quick-log widgets on the Today view (mood emoji row, weight input); unit preference
      with conversion.
- [ ] **5b.3 Day summary**: planned vs. done, missed vs. carried (shown separately), events,
      focus time, estimate vs. actual, routine streaks; browse past days in a month calendar.
- [ ] **5b.4 Trends**: small SVG chart components (line, bar, heat-strip); week/month/year
      ranges per metric; mood vs. routine-completion overlay. No chart library.
- [ ] **5b.5 Export/import and metric reminders**  *(needs Q-17)*: JSON export of all of a
      user's data; CSV per metric; CSV import for metrics if Q-17 says yes; metric reminders
      through the `Notifier`.
- [ ] Privacy test: no 5b endpoint or SSE event ever reaches another user, including group
      co-members.
- [ ] **5b.6 Phase gate**

---

### Phase 5c: Long-term goals (§6.10)

- [ ] **5c.1 Goals backend**  *(needs Q-21)*: goals, milestones, links to projects and tasks;
      derived progress (share of linked items done) with manual override; group goals via
      the ownership model.
- [ ] **5c.2 Goals UI and reviews**: goals overview with progress bars, goal detail with
      milestones and linked work; weekly/monthly review prompt that ties into reflection;
      optional weekly planning (§6.2d "later").
- [ ] **5c.3 Phase gate**

---

### Phase 6: PWA and polish (§7.6) → v1.0

- [ ] **6.1 PWA**: manifest and icons; a service worker registered **only** in a secure
      context, caching the app shell and recent API reads (stale-while-revalidate); offline
      read-only banner. Plain HTTP must keep working unchanged.
- [ ] **6.2 Notifications**: Web Push (VAPID keys generated into `data/`) as a `Notifier`
      backend when HTTPS is available; optional ntfy backend; per-user reminder settings
      covering planning, metrics, focus end and conflicts.
- [ ] **6.3 Backup and restore**: SQLite online backup (`VACUUM INTO`) on a schedule into
      `data/backups/` with retention; an admin download endpoint; restore documented in the
      README.
- [ ] **6.4 Performance and accessibility pass**: Lighthouse on a mid-range phone profile,
      bundle audit, query plans/indices for the day aggregate, accessibility audit
      (contrast, focus order, reduced motion).
- [ ] **6.5 Release v1.0**: complete README (install, proxy setups for Caddy, Traefik and
      Tailscale, backup, upgrade), CHANGELOG, tagged multi-arch image, final resource report.

---

### Phase 7: Later (after v1.0, in the order given in §7.7)

- [ ] **7.1 Integration framework + Mealie shopping list** *(needs Q-10)*: `IntegrationProvider`
      trait, encrypted settings, polling job, last-known state + warning on failure; read the
      live instance's `/docs` first. One open "Go shopping" task per list (deduplicated via
      `ext_source/ext_id`), auto-complete when the list is empty, optional minimum item count.
- [ ] **7.2 Mealie meal plan**: one task per meal entry with a recipe link, ingredients and
      steps shown in the focus view, prep+cook time as the estimate, type `expires`; generic
      "prepare lunch" tasks resolve against the meal plan.
- [ ] **7.3 Print, A4**: print-CSS day sheet with stable item codes and a QR code to the
      reconcile page.
- [ ] **7.4 Thermal printing + reconcile**  *(needs Q-13, Q-14)*: ESC/POS over TCP 9100,
      58/80 mm layouts, scheduled morning print; signed, expiring reconcile links that can only
      complete the printed items (`source=print`).
- [ ] **7.5 Child profiles** *(needs Q-15, only if needed)*: restricted role, simplified UI,
      optional parent confirmation.
- [ ] **7.6 GitHub integration**: import/link issues and PRs per project, mirror status.
- [ ] **7.7 Calendar write-back**: `put_event` on the provider; write planned blocks as busy
      events to a dedicated calendar.
- [ ] **7.8 Native app**: evaluate Tauri 2 reusing `web/`, with token auth and the
      change feed.

---

## Part C. Open questions with proposed defaults

> **Status: all defaults below accepted by the owner on 2026-10-05** (recorded as D-1…D-23
> in `DECISIONS.md`). `needs Q-x` markers in Part B now just point to the relevant decision.

These are the §9 questions. Each has a recommended default so work isn't blocked. The owner
confirms or overrides each one, and the answer goes into `DECISIONS.md`. The "Needed by"
column says when it must be settled.

| # | Question (§9) | Proposed default | Needed by |
|---|---|---|---|
| Q-1 | Default task types; custom types in v1? | One-off → `carry_on`; anchored routine → `expires`; flexible routine → `window`; tasks with due date may opt into `deadline`; workflow steps inherit from the template (default `carry_on`). Users **can** create custom types in v1, since types are just config. | 2.3 |
| Q-2 | Day-end rollover | Configurable per user, **default 04:00**. | 2.1 |
| Q-3 | Flexible routine window | **Fixed calendar periods** (ISO week with configurable week start; calendar month), not rolling. Easier to reason about and to show as "1 of 2 this week". | 3.4 |
| Q-4 | Assign to a member within a group | Not in v1. The schema keeps a nullable `assignee_user_id` so it can be added later without a migration. | 4.2 |
| Q-5 | Reminders on HTTP-only | In-app (SSE banner/toast) always; **optional ntfy** backend in Phase 6; Web Push when HTTPS is available. | 2a.3 |
| Q-6 | Prerequisite skipped or deleted | **Auto-unblock**, with a visible note on the dependent task ("prerequisite X was skipped"). `wont_do` counts the same as skipped. | 3b.1 |
| Q-7 | Routine spawns a workflow while the previous instance is unfinished | **Don't spawn**; log the occurrence as skipped ("previous run still open"). Can be changed per routine. | 3b.6 |
| Q-8 | Ritual default and reminder persistence | Default **evening**. One reminder at the planning time, plus a persistent (dismissible) banner in the Today view until the day is planned. No repeated nagging. | 2a.1 |
| Q-9 | Overbooking: warn or inform | **Warn** (amber, non-blocking). | 2a.2 |
| Q-10 | Mealie details | Watch one list chosen in settings; create the task **automatically**; **one task per meal**. | 7.1 |
| Q-11 | Meal blocked by shopping | Out of scope for v1 (users can add the dependency by hand). | 7.2 |
| Q-12 | Shareable workflow templates | JSON export/import of templates (cheap). Add in 3b.5 if time allows, otherwise in Phase 6. | 3b.5 |
| Q-13 | Thermal printer model; A4 first? | A4/browser print first (7.3). Owner names the printer model before 7.4. | 7.4 |
| Q-14 | QR reconcile link auth | Signed link that expires after 48 h and can only complete the printed items. | 7.4 |
| Q-15 | Child profiles | Postpone; decide after using printed sheets. | 7.5 |
| Q-16 | Journal encryption at rest | v1: no app-level encryption (the disk and host are trusted; documented). Revisit E2E later, since it conflicts with server-side search/export. | 5b.1 |
| Q-17 | Import historical data | Yes, a simple CSV import for metric entries (date, value). | 5b.5 |
| Q-18 | Difficulty scale; estimate format | **3 levels** (stored as 1–3, so it can grow later). Estimates stored in **minutes**, set via preset chips 5/15/30/60/120, with custom entry allowed. | 2.2 |
| Q-19 | Auto-suggest aggressiveness | **Suggest only, on request** in the wizard. Optional morning auto-draft later. | 5a.3 |
| Q-20 | Pomodoro defaults; notify when closed | 25/5/15, long break after 4, configurable per user (per task later). Notify when the app is closed only via Web Push/ntfy (Phase 6). | 2b.5 |
| Q-21 | Goal progress | **Mixed**: derived by default, manual override per goal. | 5c.1 |
| Q-22 | Eisenhower: flags or scores | Store **0–3 scores**; quadrant threshold ≥ 2. Dragging into a quadrant sets the value to 2 or 0 only if it crosses the threshold, so finer values are kept. | 2.2 |
| Q-24 | Occasions: which nameday calendar, and where do names of interest come from? | Estonian nameday calendar first (others addable as data files). Names of interest are entered by the user as *people* (name + optional birthday), and only their namedays and birthdays create tasks. Default template per kind: buy present −2 d (*deadline*) → greet 0 d (*expires*). | 3b.6a |
| Q-25 | Places: just a filter, or more? | **Answered (D-45):** a filter; pick "I'm at X" from a list, or opt in to GPS in Settings, which then detects the place automatically when the browser grants location access (HTTPS only). | 2b.6a |
| Q-23 | Units and locale | Per-user locale (date/number format) and unit system (metric default, kg). Values stored in canonical units (kg) and converted for display. | 5b.2 |

---

## Part D. Phase gate checklist (run at the end of every phase)

1. `just test` and CI green; new risky logic has unit tests in `domain` (§8 list).
2. `scripts/measure.sh` results logged in `docs/DECISIONS.md` → "Resource log" table:
   - Docker image size (target < 50 MB; aim < 25 MB)
   - Idle container RSS after 60 s (target < 50 MB; aim < 20 MB)
   - Initial frontend payload, compressed (budget < 150 KB, hard limit 300 KB)
   - p95 API latency for the day aggregate on a Pi-class machine (target < 50 ms)
3. Manual smoke test on phone + desktop, over plain HTTP and behind the HTTPS proxy.
4. No secrets in logs (grep test logs for the test passwords).
5. README and DECISIONS.md updated; OpenAPI and TS types regenerated.
6. Tag a pre-release (`v0.<phase>`), so each phase is a usable, runnable version.

---

## Part E. Risks and mitigations

| Risk | Mitigation |
|---|---|
| Rust CalDAV/iCal ecosystem gaps | Spike in 5.1 before building on it. Hand-roll the small CalDAV transport; use fixture-heavy tests. TS fallback only if parsing is truly blocked (ask the owner). |
| Recurrence + rollover + timezones get subtle | Keep them pure in `domain`, using the single `logical_date()` helper, with explicit DST tests; occurrences materialized within a bounded horizon. |
| Visibility bugs leak private or group data | One `visible_to` function used by every query and the SSE filter; the 4.1 visibility matrix test is required. |
| DnD quality on touch | Decide in 2.6 after testing on real devices; every drag also has a menu alternative. |
| Scope creep (the spec is large) | Strict phase gates; Phase 7 is explicitly after v1.0; each chunk is small enough for one session. |
| SQLite write contention with SSE + jobs | WAL, a single-writer pool, short transactions, jobs batched per tick. |
| Bundle growth | CI size budget check from 1.7 onwards. |

---

## Rough size estimate

| Phase | Chunks | Relative size |
|---|---|---|
| 1 Foundation | 9 | L |
| 2 Day plan | 7 | L |
| 2a Ritual | 4 | S |
| 2b Views & focus | 7 | M |
| 3 Recurrence | 6 | L |
| 3b Workflows | 7 | M–L |
| 4 Groups | 4 | M |
| 5 Calendar | 5 | M–L |
| 5a Blocks & planner | 4 | M |
| 5b Tracking | 6 | M |
| 5c Goals | 3 | S |
| 6 PWA & polish | 5 | M |
