# Calendar, GitHub and other integrations

All of these are set up in the app under **Settings**. Passwords and tokens are stored
encrypted (with `data/secret.key` or `SECRET_KEY`) and never sent back to the browser.

## Calendar (CalDAV)

Connect any CalDAV calendar with the same details a calendar app like Evolution or Thunderbird
asks for: Nextcloud, Fastmail, iCloud (with an app-specific password), mailbox.org, Radicale,
Baïkal and others. Google Calendar needs an OAuth sign-in, which isn't supported yet.

- Events show on the day's timeline and in the week agenda, and busy time is subtracted from
  your free time. Streamline checks for changes every 15 minutes (or *Sync now*).
- Put an event (all instances of a recurring one) into a project and attach todos to it.
- **Write-back** (optional): choose one of your calendars and your tasks with a time are written
  to it as busy events, from yesterday to two weeks ahead, kept in step within a minute. A
  separate calendar (say "Streamline") keeps them apart; turning it off removes them again.

## GitHub, GitLab and Gitea/Forgejo

Connect accounts with an access token made on the forge (GitHub, GitLab.com or self-hosted,
Gitea, Forgejo, Codeberg); several at once.

- Issues and pull/merge requests assigned to you, review requests and mentions arrive as tasks
  with a link back; each kind can be switched off per account.
- Repository rules (`acme/*` → *Work*) choose the project; anything else goes to the inbox.
- Checked every 10 minutes. A task completes itself once its issue is closed, merged or no
  longer yours. Read-only: Streamline never changes anything on the forge.
- Removing an account removes its open tasks; finished ones stay in your history.

## Notifications

Reminders, check-backs and the morning digest go to installed apps through push (needs HTTPS,
see [HTTPS](../getting-started/https.md)) and, optionally, to an [ntfy](https://ntfy.sh) topic.

## Namedays

On first start the official Estonian nameday list is downloaded (`NAMEDAYS_URL`); admins can
upload another list.
