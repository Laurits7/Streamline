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

_(Part A of WORKPLAN.md: pending confirmation.)_

## Resource log

| Phase | Image size | Idle RSS | Initial payload (gz) | Day aggregate p95 | Date |
|---|---|---|---|---|---|
