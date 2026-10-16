# Configuration

Set these in `docker-compose.yml` under `environment:`.

| Variable | Default | Meaning |
|---|---|---|
| `PORT` | `3000` | Host port in `docker-compose.yml` (the container always listens on 3000) |
| `INITIAL_ADMIN_USER` / `INITIAL_ADMIN_PASSWORD` | – | Create the first admin on startup instead of via the web UI |
| `TRUST_PROXY` | `false` | Set to `true` behind a reverse proxy, so `X-Forwarded-Proto/Host` are trusted (secure cookies over HTTPS) |
| `COOKIE_SECURE` | `auto` | `auto`, `true` or `false`. `auto` marks cookies `Secure` only when the proxy reports HTTPS |
| `SESSION_DAYS` | `90` | How long a login lasts |
| `SECRET_KEY` | – | Encrypts stored credentials (calendar passwords, GitHub/GitLab tokens, links to other servers). If unset, a random key is created in `data/secret.key`; keep it with your backups |
| `NAMEDAYS_URL` | stat.ee list | Where to download the nameday calendar on first start; `off` to never download (admins can upload a list instead) |
| `PUSH_CONTACT` | `mailto:streamline@example.org` | Contact sent to push services with notifications (use your e-mail as `mailto:you@…`) |
| `PUBLIC_URL` | – | The address Streamline is reached at (e.g. `https://todo.example.org`); used for links in notifications and on printed sheets |
| `BACKUP_HOURS` / `BACKUP_KEEP` | `24` / `7` | Automatic backups into `data/backups/` (0 hours = off) and how many to keep |
| `LOG_LEVEL` | `info` | e.g. `debug`, `info,sqlx=warn` |
| `DATA_DIR` | `/data` | Where the database lives (inside the container) |

Everything else (people, groups, calendars, integrations, notifications) is set up in the app,
under **Settings**.
