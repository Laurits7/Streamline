# Streamline

Self-hosted household todo & day planner (Rust/Axum + SQLite backend, Svelte SPA embedded in one binary/container).

- Spec: `docs/SPEC.md` (source of truth for requirements; section numbers like §6.2c refer to it).
- Plan: `docs/WORKPLAN.md` (phases → chunks with checkboxes). Work **one chunk/phase per session**, tick checkboxes as chunks land.
- Decisions: `docs/DECISIONS.md` (record confirmed answers to open questions Q-x and any deviations from the spec).
- Don't start a chunk marked `needs Q-x` until that question is confirmed in DECISIONS.md.
- Pure, risky logic (recurrence, rollover, deps, conflicts, planner) goes in `crates/domain` with tests.
