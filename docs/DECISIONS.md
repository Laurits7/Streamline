# Decisions

Short log of confirmed decisions and deviations from [`SPEC.md`](SPEC.md). Newest at the bottom.
Format: **D-n · date · topic**: decision. *Why / notes.*

## Open questions (SPEC §9)

All proposed defaults from `WORKPLAN.md` Part C were accepted by the owner on 2026-10-05.
Any of them can be revisited when the chunk that implements it starts.

- **D-1 · 2026-10-05 · Task types (Q-1)**: One-off → `carry_on`; anchored routine → `expires`; flexible routine → `window`; tasks with a due date may opt into `deadline`; workflow steps inherit from their template (default `carry_on`). Users can create custom types in v1. *Types are config rows, so custom types cost little.*
- **D-2 · 2026-10-05 · Day end (Q-2)**: Configurable per user, default 04:00.
- **D-3 · 2026-10-05 · Flexible routine window (Q-3)**: Fixed calendar periods (week with configurable week start, calendar month), not rolling windows.
- **D-4 · 2026-10-05 · Assign to group member (Q-4)**: Not in v1; nullable `assignee_user_id` kept in the schema.
- **D-5 · 2026-10-05 · Reminders on HTTP (Q-5)**: In-app (SSE) always; optional ntfy backend in Phase 6; Web Push when served over HTTPS.
- **D-6 · 2026-10-05 · Skipped/deleted prerequisite (Q-6)**: Auto-unblock with a visible note on the dependent task; `wont_do` treated like skipped.
- **D-7 · 2026-10-05 · Routine workflow while previous unfinished (Q-7)**: Don't spawn; log the occurrence as skipped ("previous run still open"). Overridable per routine.
- **D-8 · 2026-10-05 · Planning ritual default (Q-8)**: Evening. One reminder at planning time, then a dismissible banner in Today until planned. No repeated nagging.
- **D-9 · 2026-10-05 · Overbooking (Q-9)**: Warn (non-blocking).
- **D-10 · 2026-10-05 · Mealie (Q-10)**: Watch one configured list; create the shopping task automatically; one task per meal.
- **D-11 · 2026-10-05 · Meal blocked by shopping (Q-11)**: Out of scope; manual dependency possible.
- **D-12 · 2026-10-05 · Shareable workflow templates (Q-12)**: JSON export/import (3b.5 if time allows, else Phase 6).
- **D-13 · 2026-10-05 · Printing (Q-13)**: A4/browser print first; printer model to be named before chunk 7.4.
- **D-14 · 2026-10-05 · Reconcile link (Q-14)**: Signed link valid 48 h, scoped to completing the printed items only.
- **D-15 · 2026-10-05 · Child profiles (Q-15)**: Postponed until the need is real.
- **D-16 · 2026-10-05 · Journal encryption (Q-16)**: No app-level encryption in v1; host/disk trusted (documented in README). Revisit later.
- **D-17 · 2026-10-05 · Historical import (Q-17)**: CSV import for metric entries (date, value).
- **D-18 · 2026-10-05 · Difficulty & estimates (Q-18)**: 3 levels stored as 1–3; estimates in minutes via preset chips 5/15/30/60/120 plus custom entry.
- **D-19 · 2026-10-05 · Auto-suggest (Q-19)**: Suggest only, on request in the planning wizard.
- **D-20 · 2026-10-05 · Pomodoro (Q-20)**: 25/5/15, long break after 4, configurable per user; closed-app notifications only via Web Push/ntfy.
- **D-21 · 2026-10-05 · Goal progress (Q-21)**: Derived by default, manual override per goal.
- **D-22 · 2026-10-05 · Eisenhower (Q-22)**: 0–3 scores, quadrant threshold ≥ 2; dragging changes the value only when crossing the threshold.
- **D-23 · 2026-10-05 · Units & locale (Q-23)**: Per-user locale and unit system (metric/kg default); canonical storage in kg, converted for display.

## Architecture

- **D-24 · 2026-10-05 · Architecture baseline**: WORKPLAN Part A was accepted ("go"), with the deviations below made while building the MVP.
- **D-25 · 2026-10-05 · Svelte + Vite SPA instead of SvelteKit**: The app is a pure SPA embedded in the binary, so SvelteKit's routing, SSR and adapters add nothing. A ~60-line history router keeps the bundle smaller (MVP: about 41 KB gzipped in total). *Revisit if routing gets complex.*
- **D-26 · 2026-10-05 · Makefile instead of justfile**: `just` isn't installed on the dev machine, and `make` is available everywhere.
- **D-27 · 2026-10-05 · Well-known crate majors**: axum 0.8, sqlx 0.8, argon2 0.5, rand 0.8, tower-http 0.6, ulid 1. Newer majors exist (sqlx 0.9, argon2 0.6, rand 0.10, tower-http 0.7, ulid 3); upgrade them in one dedicated chunk rather than mixing it with feature work.
- **D-28 · 2026-10-05 · Dockerfile and compose at repo root**: This lets `docker compose up -d` work right after cloning. The Dockerfile avoids BuildKit-only features (cache mounts), so it builds with the classic builder too; dependencies are cached with a stub-crate layer instead.
- **D-29 · 2026-10-05 · Container runs as root, in a `scratch` image**: A bind-mounted `./data` that Docker creates is owned by root, so a non-root default would fail on first start. The image contains only the binary and CA certs. Users can set `user:` in compose if they create `data/` themselves.
- **D-30 · 2026-10-05 · First-run setup page**: If `INITIAL_ADMIN_*` isn't set, the first visitor creates the admin in the UI. No generated password is printed to logs (spec §8: never log credentials).
- **D-31 · 2026-10-05 · One active day entry per task**: Planning a task on another day *moves* its entry. Rollover moves carried entries to the new day. The history (planned, moved, snoozed, carried, missed) lives in `task_events`.
- **D-32 · 2026-10-05 · Rollover trigger**: Runs per user from a 60 s background tick and lazily on `/sync` and `/days/{date}`, guarded by `users.last_rollover_date`. If the server was down, it catches up in one step (carry count += days missed).
- **D-33 · 2026-10-05 · Sync model**: The client does one full `/sync`, then applies SSE `change` events. On every (re)connect (`hello` event) it calls `/sync?since=<rev>`. A full sync sends open tasks plus the last 30 days of history; older data is fetched on demand later.
- **D-34 · 2026-10-05 · OpenAPI** (done 2026-10-06): generated by utoipa 5 (not 6, per D-27) from `#[utoipa::path]` annotations on every handler. Served at `/api/v1/openapi.json` (public, no auth); `/api/docs` is a small page using the Scalar viewer from a CDN, so the binary ships no viewer assets. A test fails if a route is missing from the document. Cost: +0.5 MB binary.
- **D-35 · 2026-10-05 · New users start in UTC**: The web app adopts the browser's timezone once on first login (with a toast); it can be changed in Settings.
- **D-36 · 2026-10-06 · Own drag-and-drop layer** (`web/src/lib/dnd.svelte.ts`, ~300 lines, no library): built on pointer events, so mouse, touch and pen share one code path. Mouse drags start after 5 px. Touch drags start on a 350 ms long-press, so normal scrolling never starts a drag, or immediately from the grip handle. Keyboard: grip + arrow keys. A floating copy of the row follows the pointer; drop targets are any element (`droppable`) or ordered lists (`dropList`); the window and the timeline auto-scroll near their edges. Chosen over `svelte-dnd-action`, which handles list-to-list moves but not free-form targets like timeline slots or sidebar links. Cost: +3 KB gzip.
- **D-37 · 2026-10-06 · Timeline on the day view**: a 24 h grid (15-minute snapping, overlapping blocks side by side). Dropping a task sets `start_time`; the bottom-edge handle sets `duration_min` (also adjustable with arrow keys). Dropping a timed task on the plan list clears its time. Shown beside the plan on wide screens, above it on phones.
- **D-38 · 2026-10-06 · Nested projects**: Projects have an optional `parent_id` with unlimited depth (owner's request: e.g. Paper › Research / Analysis / Writing). The server rejects cycles and parents owned by someone else (`domain::tree`). Subprojects inherit the parent's colour. Archive and delete cascade to the subtree. A project page shows its own tasks plus one section per direct subproject. Counts include descendants. Tasks still belong to exactly one project.

- **D-39 · 2026-10-06 · arm64 and HTTPS**: Multi-arch images (amd64 + arm64) are built in CI with QEMU + buildx; pull requests only build them (this is what verifies arm64), while `main` and `v*` tags publish to `ghcr.io/laurits7/streamline`. Building on a Pi stays possible; `LTO`/`CODEGEN_UNITS` build args reduce compile-time RAM. The HTTPS setup is documented and tested as Streamline behind Caddy (`docs/examples/caddy`, `flush_interval -1` for SSE). Verified: Secure cookie, origin check, live events through the proxy, HTTP→HTTPS redirect, container health check.
- **D-40 · 2026-10-06 · Sync queue**: `store.sync()` runs syncs one at a time and in order. Repeated delta requests merge, but a full reload never merges into a pending delta. Responses that arrive after sign-out are dropped (found by the new store tests).
- **D-41 · 2026-10-06 · Places belong to tasks** (owner): a project's place is only a default; tasks such as "buy a garden hose" for the country home can be done elsewhere. Spec §6.16, plan 2b.6a (details in Q-25, not yet confirmed).
- **D-42 · 2026-10-06 · Occasions** (owner): the full nameday calendar is included; the user selects names of interest; each occasion has its own lead times and dependencies (e.g. buy present 2 days before → say happy birthday); task type per occasion kind with a good default. Spec §6.17, plan 3b.6a (details in Q-24, not yet confirmed).
- **D-43 · 2026-10-06 · Planning ritual details** (Phase 2a):
  - **Plan state:** a `day_plans` record per day. `draft` remembers the wizard step, so planning resumes on any device; `planned` is confirmed. No record means unplanned. Going back into the wizard keeps a planned day planned.
  - **Free time:** a new per-user "my day" window (default 08:00–22:00) minus scheduled tasks (duration, else estimate, else 30 min). For today, only the remaining part of the window counts. "Planned" is the estimates of open, unscheduled tasks; tasks without an estimate are reported, not guessed. The rules live in `domain::planning`, mirrored in `web/src/lib/planning.ts` and tested against the same cases.
  - **Reminders:** one per kind and target day (`reminder_log`), skipped when the day is already planned. A planning time before the day end (e.g. 00:30) counts for the same logical evening. Reminders are delivered in-app through `notify`, the single place where Web Push and ntfy attach later.
  - **Prompts on Today:** evening (tomorrow unplanned after the evening time) beats morning (today unplanned after the morning time), which beats a gentle "today isn't planned yet". Dismissing is per day and per browser.
  - **Wizard bundle:** loaded on demand (5 KB gzip), like Help.
- **D-44 · 2026-10-06 · Views and focus details** (Phase 2b):
  - **"In progress":** `tasks.started_at` on an open task, not a new status value (that would mean rebuilding the `tasks` table to change its CHECK). Reopening a task clears it, and starting focus on a task sets it.
  - **View prefs:** stored per scope (`view:all`, `view:inbox`, `view:project:<id>`) in `users.prefs`, a JSON object merged key by key via `PATCH /me/prefs`, so they follow the user across devices. Board and matrix pages use the wide layout, and the board auto-scrolls sideways while dragging.
  - **Board columns:** status (To do / In progress / Done in the last 2 weeks), project (inbox + top-level projects for All tasks, or the project itself + its direct subprojects, each column covering its subtree), difficulty, or task type. A drop changes that field; within a column it reorders.
  - **Matrix:** `views.ts` (quadrantOf/matrixPatch) implements D-22 and is tested over every score combination.
  - **Focus timer:** one per user, server-side (`domain::focus`). Work rolls into a break automatically; after a break the next work interval waits. Intervals abandoned within a minute aren't logged. Completed or partial work minutes are added to `tasks.actual_min`. Clients correct for clock differences with `server_now`, chime and buzz at zero (sound can be switched off), then ask the server to advance. A background job advances timers nobody is watching and sends an in-app notification; a device's own timer already signals, so those notifications only matter for other devices.
  - **Bundles:** Focus and Plan load on demand. Vite's shared chunk counts as initial in `measure.sh`, which now reads the initial set from `index.html`.
- **D-45 · 2026-10-06 · Places: manual or GPS** (owner, answers Q-25): Places act as a filter. The user picks the current place from a list. If "Use GPS" is enabled in Settings *and* the browser grants location permission, the current place is detected automatically from each place's coordinates and radius; otherwise the list is used. Note: browsers expose geolocation only in secure contexts (HTTPS or localhost), so on plain-HTTP LAN installs it is always manual.
- **D-46 · 2026-10-06 · Recurrence via the `rrule` crate** (Phase 3 spike): rrule 0.14 handled every case tested (weekdays, intervals, last day / last Friday of month, the 31st skipped in short months, Feb 29, exceptions) and shares chrono-tz with us, so nothing was hand-rolled. Occurrences are expanded as *dates* (with time of day applied later), so DST can't move a routine to another day. `UNTIL`/`COUNT` are rejected in rules; routines have their own start/end dates, which "change from a date on" needs.
- **D-47 · 2026-10-06 · Routine model** (Phase 3):
  - **Occurrences are ordinary tasks.** A routine (`series`) creates one task per occurrence. The occurrence key (a date, or window start + `#n`) is unique per routine even among deleted rows, so a skipped or deleted occurrence never comes back.
  - **When occurrences are created:** up to tomorrow (job, sync), so the evening plan sees them; for later days, on demand when they're viewed or planned. Lists hide occurrences whose date hasn't come yet.
  - **Kinds:** *repeat* is due on its dates (default Carry on). *anchored* is also put on the day's timeline at its time (default Expires). *flexible* means N slots per fixed week or month, due at the window's end (default the new built-in "Within its week/month" type).
  - **Day end:** expiring occurrences are missed even if unplanned. Open window tasks whose window ended are missed, or roll into the current window when the type's `window_overflow = roll`.
  - **Edits:** "this one" means editing the task. "All future" (`PATCH /series/{id}` with `from`) updates content in place on open occurrences from that date. A *schedule* change splits the routine: the old one ends the day before (or is deleted if it has no history), and the new one starts after the last settled occurrence or window, so nothing is done twice. Ending a routine removes open occurrences from today on and keeps the history.
  - **Mid-window changes (resolved by D-49):** changing how often a "N per week" routine runs mid-window now tops up the current window instead of waiting a week.
  - **Streaks:** consecutive done occurrences (or fully done windows). Skips don't break a streak; open, carry-on occurrences are undecided.
- **D-48 · 2026-10-06 · Owner follow-ups** (after Phase 3):
  - **Unplanned banner:** an unplanned day's banner is warm orange (warning colours), not neutral or accent.
  - **Board:** Done cards can be dragged back to To do / In progress to undo a mistaken "done". On the board only; elsewhere, closed tasks stay undraggable.
  - **Tasks in several projects:** `tasks.also_project_ids` (JSON array) next to the main `project_id`. The task is listed and counted in all of them, with one shared order. Making an "also" project the main one removes the duplicate link. Deleting a project only unlinks tasks that are merely also in it; tasks whose *main* project it is are deleted as before. Chosen over a join table: it keeps tasks a single synced record (D-33); a join table is still possible later if group sharing needs per-link data.
  - **Activity log:** `GET /days/{date}/log`, built on the fly (nothing new is stored). It includes, within the logical day:
    - task events: completed (unless later reopened), skipped, won't do
    - misses that belong to the day
    - tasks started, and tasks added by hand (not routine occurrences)
    - focus intervals
    - days marked planned

    Shown on the day view (open by default for past days) and in the planner's Review step, with "Copy as text". Groundwork for the Phase 5b daily summary.
- **D-49 · 2026-10-06 · Routine versions** (owner chose option A): a schedule change links the new routine to the one it replaced (`series.split_from`). For "N per week/month" routines the new version starts at once, and in its first window creates only the slots still missing: wanted minus those already done under earlier versions. Example: twice → three times with one done gives two new slots, shown as "1 of 3". Window progress, streaks and the client's "x/y this week" count across all versions of a routine, so streaks also survive schedule changes.
- **D-50 · 2026-10-06 · Places implementation** (2b.6a):
  - **Data:** a `places` table (name, optional lat/lon, radius 20 m–50 km). `tasks.place_id`, `projects.default_place_id` (given to new tasks in the project; the task's own place wins) and `series.place_id` (copied to each occurrence). Deleting a place makes its tasks, projects and routines "anywhere".
  - **Current place:** stored per device in localStorage, as is the GPS switch, since location and permission are per device and browser. It is never sent to the server.
  - **GPS:** uses `watchPosition`; a place matches if the position is within its radius plus the fix's accuracy (capped at 500 m), closest first. On plain HTTP, or if permission is denied, the GPS switch turns itself off with a message.
  - **Filtering:** the ready stack (pull-in, planner) and All tasks / board / matrix show tasks without a place or at the current place, with a "N at other places hidden · show all" note. Project pages and day plans stay unfiltered.

## Resource log

| Phase | Image size | Idle RSS | Initial payload (gz) | Day aggregate p95 | Date |
|---|---|---|---|---|---|
| MVP (Phase 1 + core of 2) | 7.0 MB | 4.6 MiB (after browser smoke test) | 41 KB (36.2 JS + 4.6 CSS + 0.4 HTML) | not measured | 2026-10-06 |
| End of Phase 2 | 7.5 MB | 3.6–4.1 MiB (fresh container, 20 s) | 48.0 KB initial (+4.0 KB lazy Help) | not measured | 2026-10-06 |
| End of Phase 2a | 7.6 MB | 4.0 MiB (fresh container, 20 s) | 51.2 KB initial (+4.5 Help, +5.4 Plan lazy) | not measured | 2026-10-06 |
| End of Phase 2b | 7.8 MB | 4.2 MiB (fresh container, 20 s) | 59.6 KB initial (+5.3 Help, +5.5 Plan, +2.6 Focus lazy) | not measured | 2026-10-06 |
| End of Phase 3 | 8.7 MB (+rrule) | 4.6 MiB (fresh container, 20 s) | 66.2 KB initial | not measured | 2026-10-06 |
