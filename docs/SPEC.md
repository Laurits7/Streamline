# Self-Hosted Household Todo & Day Planner: Project Spec

This document describes the product vision, constraints, architecture and build order. Read it fully before writing code. Build **one phase at a time**, keep each phase usable on its own, and ask before making decisions that contradict this spec.

## 1. Vision

A self-hosted todo and day-planning app for a household. It is deployed like Mealie: a single `docker compose up -d`, then open `http://IP:PORT` from any device (phone, tablet, computer). It combines:

- Todo lists organised **per project**
- A **day plan** where tasks are scheduled around calendar events
- **Recurring tasks, chores and routines**
- **Shared group todos** (e.g. "take out garbage" for the "Family" group, which any member can complete)
- **Calendar integration** with a CalDAV server (Radicale): read-only in v1, write-back later

## 2. Goals and non-goals

**Goals**
- Dead-simple self-hosting: one container, one data folder, easy backup.
- Works well on phone, tablet and desktop from one codebase.
- A data model that treats groups, recurrence and the day plan as first-class, not bolted on.
- Calendar provider agnostic (standard CalDAV).
- **Very responsive and modern UI**: interactions feel instant (optimistic updates, no full-page reloads, no spinners for local actions), clean contemporary design, great on touch screens.
- **Low resource usage**: idle RAM in the tens of MB, tiny Docker image, minimal CPU, suitable for a Raspberry Pi or small VPS alongside other services.

**Non-goals for v1**
- Writing events back to the calendar (design for it, don't build it).
- Native mobile apps.
- Public multi-tenant hosting, billing, or social features.
- Complex permissions beyond group membership.

## 3. Deployment requirements

- Docker image plus `docker-compose.yml`. The app is reachable at `http://IP:PORT` (port configurable via env var).
- Persistent data in a mounted volume (`./data`). Persistence is **SQLite** (single file), so backup means copying the folder.
- All configuration via environment variables and/or an in-app settings page (e.g. session secret, port, initial admin account).
- Must work over plain HTTP on a LAN. Must **also** work correctly behind a reverse proxy with HTTPS (Caddy/Traefik) or Tailscale. Do not hard-code assumptions about scheme or host.
- Note: browsers only allow service workers, true PWA install and push notifications on HTTPS (localhost excepted). The app must work fine without these over plain HTTP, and gain them automatically when served over HTTPS.
- Radicale may run in the same compose network or elsewhere. The app takes a CalDAV URL, username and password via settings.

## 4. Tech stack

Preferred stack (the owner likes Rust and wants low resource usage):

- **Backend: Rust**, using Axum (or similar), Tokio, and **SQLite via sqlx** (with migrations). Compiled to a single static binary.
- **Frontend: Svelte/SvelteKit built as a static SPA** (or SolidJS), compiled to small static assets and **embedded in or served by the Rust binary**. Result: one process, one container. The frontend is where "feels instant" is won, so use optimistic UI updates, a client-side store, and avoid heavy frameworks.
- Typed API between the two. Generate TypeScript types from the Rust types if practical (e.g. `ts-rs`, or an OpenAPI spec).
- Real-time sync between a user's devices and between group members via **SSE or WebSocket**, so a completed shared chore disappears everywhere immediately.
- **PWA** (manifest, installable, service worker where HTTPS allows).
- Calendar: server-side CalDAV client (implement with `reqwest` + PROPFIND/REPORT if no mature crate fits; Radicale's CalDAV is standard) plus iCalendar parsing and RRULE expansion (e.g. `icalendar`/`ical` and `rrule` crates). Verify crate maturity before committing and tell the owner if a piece must be hand-rolled.
- The browser never talks to Radicale directly. This avoids CORS issues and keeps credentials on the server.

If Claude Code finds a strong reason to deviate (e.g. a blocking gap in the Rust calendar ecosystem), raise it and propose an alternative rather than switching silently. A TypeScript backend is an acceptable fallback, but the owner prefers Rust.

**Resource targets (verify and report in each phase):**
- Idle memory under ~50 MB for the whole container, ideally much less.
- Docker image under ~50 MB (multi-stage build, static binary, minimal base image).
- Frontend initial payload kept small (aim for well under 300 KB compressed), with fast first paint on a mid-range phone.
- UI interactions respond in under 100 ms perceived latency.

### 4b. API-first design (future native app)

A dedicated native app is planned **after** v1 (likely Tauri 2, reusing the web frontend, though this is not decided). It will use the server instance as its backend and must **never** access the SQLite database directly. To keep that door open, from phase 1 on:

- **The web app is just one client of a documented, versioned HTTP API** (e.g. `/api/v1/...`). No business logic lives only in the frontend or in server-rendered pages.
- **Token-based auth in addition to cookies**: per-device API tokens (creatable and revocable in settings) so non-browser clients can authenticate.
- **Stable IDs**: use UUIDs or ULIDs for all entities (not auto-increment integers), so clients can create records offline.
- **Sync-friendly records**: every entity has `created_at`, `updated_at`, and soft-deletion (tombstones), so a client can later ask "what changed since X". Do not build full offline sync in v1, just don't make it impossible.
- **A real-time event stream** (SSE/WebSocket) that clients subscribe to for live updates.
- **Document the API** (OpenAPI spec generated from the code if practical).

## 5. Core concepts and data model

Design the schema around these concepts (names indicative):

- **User**: login account. Password auth with secure hashing, session cookies.
- **Group**: a set of users (e.g. "Family"). A user can belong to several groups.
- **Project**: a container of tasks. Owned by a user **or** a group.
- **Task**: title, notes, status, optional due date, optional estimated duration, optional project, owner (user) or assignee group, optional recurrence/routine definition.
- **Task type**: configuration defining end-of-day behaviour (carry on, expires, window-based, deadline-based), streak/miss handling, and overdue display. Referenced by tasks, routines and workflow steps.
- **Occurrence / completion record**: for recurring tasks, each occurrence is tracked separately, so completing today's instance doesn't affect the next. Records **who** completed it and **when**.
- **Routine**: a recurring task template with a mode (see below).
- **Task attributes**: estimated duration, difficulty, importance, urgency, actual time spent, optional external reference (source, external ID, URL).
- **Time block**: a themed range within a day (theme, start, end), part of a day plan; created from a **day plan template** or manually.
- **Day plan template**: reusable block layout, assignable to weekdays.
- **Focus session**: a Pomodoro/focus interval logged against a task (start, end, type work/break, completed or interrupted).
- **Goal**: title, description, target date, status; has **milestones** and links to projects/tasks.
- **Day plan status**: per user per date, *unplanned* or *planned* (with the time it was planned), plus the user's planning preferences (evening/morning, time).
- **Day record**: per user per date; holds the reflection/journal text and links to that day's metric entries. Created lazily.
- **Metric definition**: user-defined or built-in (mood, weight); name, type, unit, optional reminder.
- **Metric entry**: a value for a metric at a date/time, owned by one user. Private to that user.
- **Dependency (prerequisite)**: a link "task B is blocked by task A". Forms a directed acyclic graph (reject cycles). A task is **ready** only when all its prerequisites are complete.
- **Workflow template**: a reusable multi-step definition (steps, their order/dependencies, optional variants). Instantiating it creates real tasks with dependencies already wired up. Can be attached to a routine so every occurrence spawns a fresh instance.
- **Day plan entry**: places a task (or occurrence) into a specific day, optionally at a specific time or in an ordered position. Separate from the task itself.
- **Calendar source**: CalDAV connection settings (URL, credentials, selected calendars) and cached events with sync state.

## 6. Feature requirements

### 6.1 Projects and todos
- Create/edit/archive projects. Each project has its own todo list.
- Tasks can be added, edited, reordered, completed, and moved between projects.
- A quick-add inbox for tasks without a project.

### 6.2 Day plan
- **The default landing view of the app is the day view** ("what to expect today"): upcoming calendar events, routines and tasks due today in order, blocked/waiting items clearly separated, and what is coming next. The user should open the app and immediately see their day, with other views (projects, kanban, goals, etc.) one tap away.
- A "Today" view that shows: calendar events (fixed), routines due today, and tasks the user has pulled in.
- Tasks live in projects and are **pulled into** the day plan, which is a separate layer, so projects stay clean and the daily view stays flexible.
- Tasks can be ordered within the day, and optionally given a time slot or duration.
- Show free time between calendar events where helpful.
- What happens to unfinished items at day end is decided by the **task type** (see 6.2c), not by one global rule.

### 6.2d Daily planning ritual
The app should actively support **planning the next day** as a regular habit, either in the **evening (plan tomorrow)** or in the **morning (plan today)**. The user chooses which in settings (or both: a quick evening plan, a morning check).
- **Guided planning flow** (a short, fast wizard, not a form-heavy screen):
  1. **Review**: what was completed, missed and carried over from the day just ended (ties into the daily summary, 6.2b).
  2. **Look ahead**: the target day's calendar events, routines and anchored items that are already fixed, and anything due or overdue.
  3. **Pick**: pull tasks from the ready stack into the day (blocked tasks excluded), guided by estimates and difficulty. Show a running total of planned time against available free time, with a warning when the day is overbooked.
  4. **Arrange**: order tasks or assign them to themed blocks (6.8). From phase 5a on, offer the deterministic auto-suggested plan as a starting point.
  5. **Confirm**: mark the day as planned.
- **Evening flow can chain into reflection**: reflect on the day (6.2b), then plan tomorrow, in one continuous flow.
- **Plan status**: each day has a state (*unplanned* / *planned*). An unplanned day is visible in the day view with a clear prompt to plan it, and the user is **reminded** at their configured planning time (in-app, and push where HTTPS is available).
- **Fallback when no plan was made**: the day view still works by default, showing anchored routines, calendar events, due and carried-over tasks, so the app is useful even if the ritual is skipped. A plan is encouraged, not required.
- **Weekly planning (optional, later)**: a similar lightweight review of the week ahead and the past week, tying into goals (6.10).
- The planning time is configurable (e.g. 21:00 for the evening, 07:30 for the morning) and respects the user's configured "day end" rollover.
- Planning is personal by default. Group tasks assigned to a group appear in the ready stack for any member to pick up when planning.

### 6.2c Task types and end-of-day behaviour
Tasks (and routines/workflow steps) have a **type** that defines how they behave when not completed. Types are user-configurable, with sensible built-in defaults:

- **Carry on** (e.g. "call the dentist", "fix the bike", project work): if not done, it stays on the list and rolls over to the next day (or returns to the ready stack) until completed. Optionally shows how many days it has been carried over.
- **Expires / daily-only** (e.g. "brush teeth", "take vitamins", "water plants" if only meaningful that day): if not done by day end, it is marked **missed** and does **not** carry over. The next occurrence appears fresh. Missed items are recorded for stats and streaks (not silently deleted).
- **Window-based** (e.g. flexible routines like "laundry twice a week"): carries on within its window (week/month), then counts as missed or rolls into the next window according to its configuration.
- **Deadline-based**: carries on until its due date, then is flagged overdue and stays visible until resolved.

Requirements:
- Each task type is a small configuration (carry-over behaviour, whether misses count against streaks, whether it shows as overdue), not hard-coded logic scattered around the app. New types should be addable without code changes where possible.
- The task type is chosen when creating a task or routine, with a sensible default (plain one-off tasks default to carry on; anchored routines default to expires).
- The behaviour applies consistently to recurring occurrences, group tasks, and workflow steps. Workflow steps that are blocked do not "miss" while blocked.
- The daily summary (6.2b) must show missed vs. carried-over items distinctly.
- A **user can always override** per item: skip today, snooze to a date, or mark "won't do", independent of type.

### 6.2b Daily summary, reflection and tracking
- Every day has a **summary view**: what was planned vs. completed (tasks, routines, workflow steps), calendar events attended, what was carried over, and simple stats (e.g. completion rate, streaks for routines).
- A **reflection area** per day: free-text journal entry, plus an optional prompt-style layout (e.g. "what went well / what didn't / tomorrow"). Keep it fast to fill in on a phone.
- **Daily metrics tracking**:
  - Built-in: **mood** (simple scale, optional emoji) and **weight**.
  - **User-defined metrics** (e.g. sleep hours, water, energy, steps, symptoms): each with a name, type (number, scale, yes/no), unit, and optional daily reminder.
  - Metrics can be logged more than once a day if needed (e.g. weight), with the day view showing the latest or an average.
- **Trends**: simple charts over time (week/month/year) for each metric, plus an overview of mood vs. routine completion. Keep charts lightweight (no heavy charting library; small SVG-based rendering is fine).
- The summary can be viewed live for today and browsed for past days (calendar-style navigation).
- **Privacy:** reflections, mood, weight and other metrics are **personal by default** and never visible to other group members, even in shared groups. Sharing, if ever added, must be explicit and opt-in.
- Data is exportable (JSON/CSV), since this is personal long-term data.

### 6.3 Recurring tasks and routines
- Use the **RRULE standard** for recurrence (same format calendars use).
- Two routine modes:
  - **Flexible**: "needs doing N times per period, it doesn't matter when" (e.g. laundry twice a week, vacuum weekly). Shows as due within the window, can be done any day.
  - **Anchored**: "every morning at 07:00 do X". Appears at a fixed time on the day plan.
- Each occurrence has its own completion record. Missing one occurrence does not corrupt the series.
- Editing a series vs. a single occurrence must be handled deliberately ("this one" vs "all future").

### 6.3b Prerequisites and multi-step workflows
- A task can have **prerequisites**. Until all are complete, the task is **blocked**: it is **not** shown in the general/"ready" todo stack and cannot be pulled into the day plan. It remains visible in its project (clearly marked as blocked, showing what it is waiting on).
- The moment the last prerequisite is completed, the next task becomes ready and appears in the general stack (and notifies/updates live on all devices).
- **Multi-step workflows**: a todo can consist of ordered steps, each its own task with dependencies. Example, laundry:
  - Workflow "Laundry" with **variants** per clothing type (colourful, whites, delicates, towels, bedding...).
  - Each variant is a chain such as: *Wash* → *Dry* (or *Hang to dry*) → *Iron* (only for some types) → *Fold / put away*.
  - Steps can differ per variant (e.g. delicates skip the dryer, towels skip ironing).
  - Starting "Laundry" lets the user pick which variants/loads to run, and creates one chain per selected load.
  - Only the currently ready step of each chain shows up in the general stack, so the stack stays short and actionable.
- Optional **wait time** on a step ("machine runs ~1 h"): the following step becomes ready only after the previous one is done *and* the wait has elapsed (or the user can mark it done manually). Keep this optional and simple.
- Workflows can be one-off or attached to a recurring routine, so each recurrence spawns a fresh chain.
- Blocking must work across projects and for group tasks (a group chore can be blocked by another group chore).
- Provide a simple visual for a chain's progress (e.g. step indicators), not a complex graph editor, in v1.

### 6.4 Groups and shared tasks
- A task can be assigned to a **group** rather than a person. Every group member sees it, any member can complete it, and the completion records who did.
- Group projects are visible to all members.
- Recurring group chores (e.g. "take out garbage" for Family) work with the same recurrence system.
- Simple group management: create group, invite/add existing users (self-hosted, so admin-driven is fine).

### 6.5 Calendar integration (CalDAV / Radicale)
- **v1: read only.** Settings page for the CalDAV URL, username, password, and which calendars to include. Server syncs events periodically and on demand, and caches them.
- Handle recurring events (expand RRULE), all-day events, and time zones correctly.
- Events appear in the day plan and a simple agenda/week view.
- **Later (not v1):** write tasks or day-plan blocks back to a calendar. Keep the sync layer abstract enough (an interface for a calendar provider) so writing can be added without a redesign.

### 6.6 Task attributes: difficulty, estimates, priority
- Every task can have an **estimated duration** and a **difficulty/effort level** (e.g. easy / medium / hard, or a 1-5 scale), plus an **importance** and **urgency** flag or score (used by the Eisenhower view).
- Estimates and difficulty are optional, quick to set, and used by the planner (6.8), the focus view (6.9) and the daily summary (planned vs. actual time).
- Optionally record **actual time spent** (fed automatically by the focus timer) so estimates can be compared to reality over time.

### 6.7 Multiple views of the same tasks
The same underlying tasks can be shown in several interchangeable views. Views are presentation only, never separate data.
- **List** (default, per project and the general ready stack).
- **Kanban** (columns by status, or by project/type; drag between columns).
- **Block view** (day divided into themed blocks, see 6.8).
- **Eisenhower matrix** (urgent/important quadrants). Dragging a task between quadrants updates its urgency/importance. Blocked tasks (prerequisites) are excluded or shown dimmed.
- Views can be switched at any time and each remembers the user's filters.

### 6.8 Block planning and automatic day plan templates
- A day can be divided into **time blocks**, each with a **theme** (e.g. "Deep work: hard tasks", "Admin: easy tasks", "Chores", "Wind-down") and a time range.
- **Day plan templates**: reusable block layouts (e.g. "Workday", "Weekend", "Light day"), assignable per weekday or applied manually. Templates are user-editable.
- **Auto-suggest a plan**: given the template, the day's calendar events, and the ready tasks, the app proposes which tasks go into which block, using difficulty, estimated duration, importance/urgency and deadlines (e.g. harder tasks in earlier blocks, easier later). The user can accept, adjust or reject the suggestion. It must be **deterministic and explainable** (show why a task was placed), not a black box. No AI/LLM dependency in v1.
- The planner only schedules **ready** tasks (not blocked by prerequisites) and respects task types (6.2c).

### 6.9 Focus view and Pomodoro timer
- A **focus view** for a single task: shows its title, full description/notes, checklist or workflow step context, and a distraction-free layout.
- Built-in **Pomodoro timer** (configurable work/break lengths, long break after N sessions), with start/pause/skip, audio/vibration cue, and a persistent mini-timer if the user leaves the focus view.
- The timer's state lives **on the server** (or is reconstructable from it), so it stays correct across devices and page reloads (start on phone, see it on desktop).
- Completed focus sessions are logged against the task (feeding actual time spent, 6.6) and show up in the daily summary.
- From the focus view the user can mark the task done, snooze it, or move on to the next planned task.

### 6.10 Long-term goals
- **Goals** with a title, description, optional target date, and a status.
- Goals can be broken down into **milestones** and linked to projects and tasks, so everyday work visibly contributes to long-term goals.
- Progress is derived where possible (e.g. share of linked tasks/milestones completed) and can be manually overridden.
- A goals overview, plus a periodic **review** prompt (weekly/monthly) that can tie into the daily reflection (6.2b).
- Goals are personal by default; group goals can follow the group-ownership model.

### 6.11 Calendar conflict detection and time blocking
- Scheduled tasks and planned blocks **occupy time** in the day plan. The app must **detect double-booking**: a scheduled task or block overlapping a calendar event, or two scheduled items overlapping each other.
- On conflict, show a clear **alert** (in the day plan and as a notification where available), and offer quick resolutions (move, shorten, unschedule).
- v1 (calendar is read-only): conflicts are detected inside the app against the synced events. The planner and auto-suggest never place tasks on top of existing events.
- Later, once write-back exists: optionally write planned blocks to a dedicated calendar in Radicale as busy events, so other calendar clients see the time as blocked. Keep this behind the calendar provider abstraction (6.5).

### 6.12 Drag and drop
- Drag and drop works **everywhere it makes sense**: reordering tasks in a list, moving tasks between projects, between kanban columns, between Eisenhower quadrants, from the ready stack into day-plan blocks or time slots, and rearranging blocks.
- Must work well with **touch** (long-press to drag, no accidental drags while scrolling) and with the **keyboard** (accessible alternative to dragging). Moves are optimistic and instant, and sync live to other devices.
- Every drag action must also have a non-drag alternative (menu or buttons), for accessibility and for small screens.

### 6.13 Integrations (very late stage)
- **GitHub integration** (issues, pull requests, etc.): late in the project, not in v1. Idea: link or import GitHub issues/PRs as tasks (read-first, two-way sync later), per project, with status mirrored. This is recorded here so it is not forgotten. Design the task model so tasks can carry an **external reference** (source, external ID, URL) from the start; do not build the integration itself until much later.

### 6.14 Printable day sheet (receipt printer / paper), "offline" checklists
An out-of-the-box idea, **not for early phases**: print the day's task list on paper so it can be ticked off without a screen (e.g. for a child), then reconciled digitally later.
- **Printable day sheet**: a print-friendly rendering of a day plan (or one person's list, or a group's chores). Two targets: a normal **browser print/PDF layout** (A4 or similar, via print CSS) as the easy first step, and **thermal receipt printers** (58/80 mm, ESC/POS) as the fancy step.
- **Thermal printing from the server**: the server (running in Docker) sends ESC/POS to a network printer on the LAN (typically TCP port 9100) or via a supported driver. Configurable in settings (printer address, paper width). Optional: print automatically each morning at a set time.
- **Each printed line carries a stable item identifier**, and the sheet has a **QR code** linking to a lightweight "reconcile" page for that sheet. Scanning it on a phone opens a checklist of exactly the printed items, so ticking things off that were done on paper takes seconds. No handwriting recognition or OCR.
- The reconcile page must be usable **without a full login** where it makes sense (e.g. a signed, expiring link tied to that sheet), but only able to mark those printed items as done, nothing else.
- **Child-friendly profiles** (idea): a simple user profile for a child with a limited view (their chores/routines only, big friendly layout, no access to journals or settings) and optionally a parent confirmation step ("child ticked it, parent confirms"). Keep it minimal until the need is real.
- Completion via the reconcile page records **who/when** like any other completion, with a note that it came from a printed sheet.

### 6.15 App integrations: Mealie first (and other self-hosted apps)
The app can query other self-hosted apps over their APIs and turn what it finds into tasks. **Mealie** is the first target. This is a later phase, but the design must keep the door open (the task model already carries an external reference, see 6.6).
- **Integration framework**: a small provider abstraction (like the calendar one in 6.5). Each integration has its own settings (base URL, API token), is configured **server-side only**, and never exposes credentials to the browser. Sync is by polling on an interval and on demand; use webhooks only if the other app offers them and it is simple. Integrations are **read-only** in the first version.
- **Mealie: shopping list to task**:
  - If a chosen Mealie shopping list has unchecked items, the app creates (or keeps) a task such as "Go shopping", linked to that list, showing the item count and the items in the task details.
  - It must **not create duplicates**: one open task per list. When the list is emptied or fully checked in Mealie, the task is completed or removed automatically (configurable), and the user can also complete it manually.
  - Optional rule: only create the task once the list has at least N items.
- **Mealie: meal plan to task**:
  - Meal plan entries for a day (breakfast, lunch, dinner) become tasks such as "Prepare lunch: <recipe name>", with a link to the recipe, the ingredients and instructions viewable in the task and in the **focus view** (6.9), and the recipe's prep/cook time used as the **estimated duration** (6.6).
  - A generic user-created task or routine like "prepare lunch" can **resolve against the meal plan**: when the day's lunch is planned in Mealie, the task shows that recipe; if nothing is planned, it stays a plain task.
  - These tasks use sensible task types (6.2c). For example, a meal is typically *expires* (that day only) rather than carry on.
- **Task source labelling**: tasks coming from an integration are clearly marked with their source and link back to the original item.
- **Failure handling**: if the other app is unreachable, show the last known state and a small warning, never block the rest of the app.
- Design the framework so other self-hosted apps can be added later (ideas: Grocy, Home Assistant, Nextcloud, Paperless-ngx) without changing the core. Do not build any of those now.
- **Implementation note for Claude Code:** check the running Mealie instance's own OpenAPI docs (its `/docs` page) for the current endpoints and authentication, rather than relying on remembered endpoint names, since they change between versions.

## 7. Build phases

Build and verify each phase before starting the next. Each should end with working, runnable software.

1. **Foundation**: repo, Dockerfile, compose file, SQLite + migrations, auth (users, sessions), projects and tasks CRUD, responsive UI shell.
2. **Day plan**: Today view, pulling tasks into a day, ordering, time slots, **task attributes** (estimate, difficulty, importance/urgency, see 6.6), basic drag and drop, and **task types with end-of-day behaviour** (carry on vs. expires; add the remaining types as their features arrive).
2a. **Planning ritual (basic)**: guided plan-the-day flow (review, look ahead, pick, arrange, confirm), planned/unplanned state, planning reminder, fallback day view. Manual picking first; auto-suggest joins in phase 5a.
2b. **Views and focus**: kanban view, Eisenhower matrix, drag and drop across all views (6.7, 6.12), focus view with Pomodoro timer and session logging (6.9).
3. **Recurrence and routines**: RRULE-based recurrence, per-occurrence completion, flexible and anchored routines.
3b. **Prerequisites and workflows**: dependency graph with cycle detection, ready/blocked logic, workflow templates with variants, spawning chains from routines.
4. **Groups**: groups, group-owned projects, group-assigned tasks, who-completed tracking, live sync between members.
5. **Calendar read**: CalDAV sync with Radicale, event display in day plan and agenda.
5a. **Conflicts and block planning**: double-booking detection and alerts (6.11), themed time blocks, day plan templates, deterministic auto-suggested plan (6.8).
5b. **Daily summary and tracking**: day summary view, reflection/journal, mood, weight, custom metrics, trend charts, export. (The summary gets richer as earlier phases land, so a basic version can be started earlier, but build it fully after recurrence and workflows exist.)
5c. **Long-term goals**: goals, milestones, linking to projects/tasks, progress and review prompts (6.10).
6. **PWA and polish**: manifest, service worker, offline basics, reminders/notifications (where HTTPS is available), backup/export, README.
7. **Later (after the core is stable), roughly in this order:** app integrations, Mealie first (6.15); then printable day sheets and receipt printing with QR reconcile (6.14), GitHub integration (issues/PRs, 6.13), calendar write-back, the dedicated native app (4b).

## 8. Working agreements for Claude Code

- Start by proposing a short project plan and folder structure, and wait for confirmation before scaffolding.
- Work one phase per session to keep context small. Update a `README.md` and a short `docs/DECISIONS.md` as you go.
- Write tests for the logic most likely to break: recurrence expansion, occurrence completion, dependency/ready-blocked logic and cycle rejection, workflow instantiation, group visibility rules, calendar parsing and time zones, double-booking/conflict detection, and the auto-suggest planner (deterministic, so it is testable).
- Prefer boring, well-maintained dependencies. Avoid anything that complicates the single-container deployment.
- Never log or expose credentials. Store the CalDAV password server-side only.
- Flag ambiguities and trade-offs rather than silently picking, especially around recurrence semantics and day carry-over behaviour.

## 9. Open questions to settle early

- Exact default task type per kind of item, and whether users can define fully custom types or only pick from the built-in set in v1.
- When does "day end" roll over (midnight, or a configurable time such as 04:00 for late-night users)?
- Exact semantics of flexible routines (rolling window vs. fixed calendar week).
- Whether group assignment should later allow "assign to a specific member within a group".
- Reminder delivery method if the instance is HTTP-only (in-app only vs. optional external push such as ntfy).
- If a prerequisite is skipped or deleted, what happens to blocked tasks (auto-unblock, ask, or keep blocked)?
- Should a recurring routine spawn a new workflow instance if the previous instance is still unfinished?
- Planning ritual: evening, morning, or both as the default, and how persistent the reminder should be if a day stays unplanned.
- Whether the planning flow should warn on overbooking (planned time exceeding free time) or only inform.
- Mealie: which shopping list(s) to watch, whether the "go shopping" task should be created automatically or only suggested, and how meal plan entries should map to tasks (one per meal vs. one per day).
- Whether a meal task should be able to be *blocked by* a shopping task (needs a way to know about missing ingredients, probably out of scope).
- Whether workflow templates should be shareable/exportable between instances.
- Printing: which thermal printer model(s) will be used (decides the ESC/POS details), and whether to start with a plain A4/browser print layout first.
- Printing: should the QR reconcile link be a short-lived signed link or require a normal login?
- Whether child profiles are needed (limited view, parent confirmation) or if printed sheets for a regular user are enough.
- Whether journal entries should be encrypted at rest (server-side or end-to-end), given how personal they are.
- Whether to support importing historical data (e.g. weight from a CSV or another app).
- Difficulty scale (3 levels vs. 1-5) and whether estimates use free minutes or preset sizes (e.g. 15/30/60/120 min).
- Auto-suggest planner: how aggressive should it be (suggest only, or auto-fill the day each morning for approval)?
- Pomodoro defaults (25/5/15 classic or configurable per task) and whether the timer should notify when the app is closed (needs HTTPS push).
- Goals: should goal progress be purely derived, or always manually set, or a mix?
- Whether the Eisenhower matrix uses binary flags or numeric scores for urgency/importance.
- Units and locale handling for metrics (kg/lb, date formats).
