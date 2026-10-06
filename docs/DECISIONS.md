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
- **D-34 · 2026-10-05 · Deferred from Phase 1**: The OpenAPI document (utoipa) isn't in the MVP; the API is documented in README for now. Tracked in WORKPLAN 1.5.
- **D-35 · 2026-10-05 · New users start in UTC**: The web app adopts the browser's timezone once on first login (with a toast); it can be changed in Settings.
- **D-36 · 2026-10-06 · Own drag-and-drop layer** (`web/src/lib/dnd.svelte.ts`, ~300 lines, no library): built on pointer events, so mouse, touch and pen share one code path. Mouse drags start after 5 px. Touch drags start on a 350 ms long-press, so normal scrolling never starts a drag, or immediately from the grip handle. Keyboard: grip + arrow keys. A floating copy of the row follows the pointer; drop targets are any element (`droppable`) or ordered lists (`dropList`); the window and the timeline auto-scroll near their edges. Chosen over `svelte-dnd-action`, which handles list-to-list moves but not free-form targets like timeline slots or sidebar links. Cost: +3 KB gzip.
- **D-37 · 2026-10-06 · Timeline on the day view**: a 24 h grid (15-minute snapping, overlapping blocks side by side). Dropping a task sets `start_time`; the bottom-edge handle sets `duration_min` (also adjustable with arrow keys). Dropping a timed task on the plan list clears its time. Shown beside the plan on wide screens, above it on phones.
- **D-38 · 2026-10-06 · Nested projects**: Projects have an optional `parent_id` with unlimited depth (owner's request: e.g. Paper › Research / Analysis / Writing). The server rejects cycles and parents owned by someone else (`domain::tree`). Subprojects inherit the parent's colour. Archive and delete cascade to the subtree. A project page shows its own tasks plus one section per direct subproject. Counts include descendants. Tasks still belong to exactly one project.

## Resource log

| Phase | Image size | Idle RSS | Initial payload (gz) | Day aggregate p95 | Date |
|---|---|---|---|---|---|
| MVP (Phase 1 + core of 2) | 7.0 MB | 4.6 MiB (after browser smoke test) | 41 KB (36.2 JS + 4.6 CSS + 0.4 HTML) | not measured | 2026-10-06 |
