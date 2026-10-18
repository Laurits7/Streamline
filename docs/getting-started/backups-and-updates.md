# Backups and updates

## Backups

Streamline copies its database to `data/backups/` every day and keeps the newest 7 (change with
`BACKUP_HOURS` / `BACKUP_KEEP`; `BACKUP_HOURS=0` turns it off). Admins can make one now and
download backups in **Settings → Backups**.

Also copy the whole `data/` folder somewhere else from time to time. Besides the database it
holds:

- `secret.key`: needed to read stored calendar passwords, GitHub/GitLab tokens and links to
  other servers;
- `vapid.key`: push notifications.

## Restore

1. Stop the container.
2. Copy the backup to `data/streamline.db`.
3. Delete `data/streamline.db-wal` and `data/streamline.db-shm` if present.
4. Start it again.

## Updating

Make a backup first (**Settings → Backups → Back up now**, or copy `data/`), then:

=== "Prebuilt image"

    ```sh
    docker compose pull && docker compose up -d
    ```

=== "From source"

    ```sh
    git pull && docker compose up -d --build
    ```

Pin a version with `image: ghcr.io/laurits7/streamline:1.5.0` instead of `latest` if you prefer
to update on your own schedule. Database migrations run automatically on startup; going back to
an older version after a migration means restoring the backup.
