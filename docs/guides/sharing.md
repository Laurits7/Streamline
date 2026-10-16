# Sharing with other Streamline servers

People don't need an account on your server to work on a project with you: if they run their
own Streamline, share the project from yours.

## Share a project

1. Share a top-level project with a group you own (project menu → *Shared with*).
2. Open the project menu → **People on other servers** → **Make an invite link**.
3. Send the link to that one person. It works once, for 7 days.

## Join one

Paste the link you were sent in your own Streamline under **Settings → Projects from other
servers**. The project appears with your own, marked *From* the other server, and you work on
it as usual: plan its tasks on your days, tick them off, add tasks, steps and subprojects.

## How it works

Their server keeps a copy of the project (subprojects, tasks, routines) and sends their changes
to yours, which checks them as it would for anyone in the group. Changes arrive on the other
side within moments.

- **Personal data stays home**: day plans, journals, tracking, calendars and focus time stay on
  each person's own server. Places, workflows and custom task types aren't shared.
- **Reachability**: your server must be reachable from theirs (over HTTPS when it's on the
  internet, see [HTTPS](../getting-started/https.md)). Theirs never needs to be reachable: it
  only connects out to yours.
- **When your server is down**, their copy is read-only and says so.
- **Ending it**: remove someone from the same menu, or take the project out of the group; they
  can also leave from their Settings. Either way their copy disappears.

## Try it on one machine

```sh
docker compose -f docker/docker-compose.two-servers.yml up --build
```

This starts two servers at <http://localhost:3001> and <http://localhost:3002>.
