# API

All endpoints are under `/api/v1` and use JSON. The full, always-current description is the
OpenAPI document at `/api/v1/openapi.json` (generated from the code); read it at `/api/docs`
on your server (the viewer loads from a CDN).

## Authentication

Use the session cookie (web app) or `Authorization: Bearer <token>` with a token created in
**Settings → API tokens**.

## The main endpoints

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

## Conventions

- Clients may choose [ULID](https://github.com/ulid/spec) ids when creating records, so they can
  work optimistically or offline. Retrying a create with the same id is safe.
- TypeScript types for every response are in
  [`web/src/lib/api/types`](https://github.com/Laurits7/Streamline/tree/main/web/src/lib/api/types),
  generated from the Rust structs.
