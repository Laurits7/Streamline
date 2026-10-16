# Security

- **First run:** the first person to open a fresh install creates the admin account. If the
  server is reachable by others before you've done that, set `INITIAL_ADMIN_USER` and
  `INITIAL_ADMIN_PASSWORD` instead.
- **Accounts:** passwords are hashed with Argon2; sign-in attempts are rate-limited; sessions
  and API tokens can be revoked (changing your password revokes your API tokens).
- **Stored credentials** (calendar passwords, forge tokens, links to other servers) are
  encrypted with `data/secret.key` (or `SECRET_KEY`).
- **Web hardening:** responses carry a strict Content-Security-Policy and no-framing headers;
  state-changing requests are checked against the page's origin.
- **Outgoing requests** go only to addresses you give it (calendar, ntfy, forges, other
  Streamline servers) and to push services. Local network addresses are allowed (for a home
  calendar server); cloud-metadata and link-local addresses are refused, and redirects never
  leave the host.
- **Privacy in groups:** journal, tracking, day plans, habit answers and calendar events are
  visible only to their owner, even inside a shared group. A shared item can be taken private
  or deleted by any member of its group.
- **Sharing with other servers:** the invite link is the credential (single use, 7 days); the
  other server then holds a per-link key that only reaches that project and can be cut off at
  any time. The name and server shown for someone who joined are what their server says.
- **Printed sheets:** the QR link can only tick off that sheet's open tasks and expires after
  48 hours.
- Keep `data/` private: it holds the database, backups and keys. Use HTTPS when the server is
  reachable from outside your home network.
