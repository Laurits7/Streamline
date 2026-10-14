# Changelog

## Unreleased (1.3.0)

- Checklists complete their task: ticking the last step completes it (with Undo), and
  unticking a step of a completed task reopens it. Works the same for routines and on every
  device (D-80).
- Checklists on the timeline and in the Agenda: hover a task on the timeline (long-press on a
  phone, or its 2/4 chip with the keyboard) to tick its steps in a small popover; the Agenda
  shows each task's steps under it, tickable (D-81).
- Checklist steps show in the day log on the day they were done, so a chore spread over two
  days (wash on Monday, fold on Tuesday) shows up on both (D-84). On the timeline the 2/4
  chip moved to the second line so the title has the whole first line.
- Default tasks can have kinds: Laundry → linen, colours; garbage → bio, paper. Picking one
  asks which kinds this time and adds a task for each, with the kind's own steps or the
  default task's (D-86).
- "Add a task…" shows your default tasks as soon as you click into it, narrows them as you
  type (kind names too), and lets you pick with the arrow keys; Enter without a pick still
  adds what you typed.
- Linux desktop app (first part; packages come later in 1.3): Streamline in its own
  window, connected to your server with a per-computer key kept in the system keyring. It
  stays in the tray when closed and can start at login (Settings → Desktop app). Build it
  with `make desktop` (D-78, D-87).
- The desktop app works offline: it keeps a copy of your data, and changes made offline
  wait on the computer and are sent when the server is back, merged field by field with
  changes made elsewhere; a note says if anything clashed or was dropped (D-79, D-89).
- The desktop app can also run on its own, without a server ("This computer only" on first
  start), for a single laptop. Your data stays in a folder that can later be used to start
  a real Streamline server (D-88).
- API: updates can carry `base` (the values they were based on) and `edited_at`; the server
  merges them field by field with newer changes (checklists per item) and lists any
  `conflicts` in the response. Groundwork for the offline desktop app (D-79, D-85).

## 1.2.0 (2026-10-08)

Upgrading: back up first (Settings → Backups, or copy `data/`). This version migrates the
database; 1.0.0 can't open it afterwards, so going back means restoring the backup.

- Guided tour: on first sign-in a short walk through the app (adding tasks, the timeline, the
  plan, planning the day, tracking, the inbox, projects, routines, the agenda, reflection and
  settings), highlighting each in place. Offered once; replay it from Help or Settings (D-77).
- Health check-in: once a day, feeling great / OK / not feeling well / sick (what it is) /
  injured (where), with an optional note. On the Today view, the planner's reflection step
  and the summary (past days too); sick and injured days are marked on the month calendar,
  Trends has a health strip. Personal, only a record, included in the export (D-75).
- Default tasks: ready-made tasks (title, checklist, estimate, project and more), personal or
  shared with a group. "Save as default task" in a task, Settings → Default tasks to manage
  them; every "Add a task…" field suggests them as you type and lists them with a button.
  Included in the data export (D-71).
- Checklists: a task can hold its steps (Do laundry: sort, wash, hang, fold), ticked in the
  task sheet or the focus view, with progress (2/4) on rows, board cards and the timeline.
  Ticking the last step offers to complete the task. Routines can carry a checklist; each
  occurrence starts unticked (D-71).
- Waiting for results: a task whose work is handed off (a model training, a reply to an
  e-mail) can wait, with what it waits for and when to check back (quick choices or a time).
  It needs no action meanwhile: out of Up next, the ready stack, the planner and overlaps,
  never missed, carried on. Today has a *Waiting for results* section, the board a *Waiting*
  column, the timeline a check-back marker. At the check-back time a reminder is sent and the
  task returns to the top of Up next (D-70).
- Every project has a colour and a shape (circle, square, triangle, diamond, hexagon, star),
  picked automatically so each looks different; subprojects keep their parent's colour with
  their own shape. Existing projects get theirs on upgrade. Ideas show an empty grey circle
  (D-74).
- Ideas can be added inside active projects (D-72).
- Every project has a description (plain text; web links are clickable) (D-72).
- Project ideas: a project can be *just an idea for now*. It stays in the Projects list with a grey
  "Idea" tag and can hold a description, tasks and subprojects, but asks for no attention: its
  tasks stay out of Today, planning, All tasks and reminders, and are never missed or carried
  over; its routines don't run. **Activate** starts it (D-72).
- New logo: a stone in a stream, with the water parting around it. Used for the browser tab,
  the installed app, notifications, the sidebar and sign-in; the accent colour is now the
  logo's blue (D-73).

## 1.0.0 (2026-10-07)

The first complete version. Built in phases (see `docs/WORKPLAN.md`); decisions are in
`docs/DECISIONS.md` (D-numbers below).

### Day planning
- Today view: timeline with a "now" line, an ordered plan, up next, due and overdue tasks,
  progress, other days. A three-column layout on wide screens.
- Pull tasks into a day, give them a time and duration by drag and drop (mouse, touch,
  keyboard) or menus.
- Task types decide what happens at the end of the day: carry on, expire (missed), within a
  window, or deadline. Configurable day end (default 04:00).
- Planning ritual: review → look ahead → pick → arrange → confirm, with a reminder and a
  banner until the day is planned. Free time vs. planned time with an overbooking warning.
- Time blocks and day templates per weekday; "Suggest times" proposes a plan with reasons
  (deterministic, never on busy time) (D-58).
- Overlap detection with calendar events and other tasks, with quick fixes and notifications.

### Tasks and projects
- Projects with subprojects of any depth, an inbox, tasks in several projects.
- List, board and urgent/important matrix views; drag between columns and quadrants.
- Estimates, difficulty, importance and urgency; places with optional GPS detection.
- Focus view with a server-side Pomodoro timer and a mini timer.
- Daily activity log.

### Routines and chores
- Repeating tasks (any iCalendar rule), fixed-time routines on the timeline, "N times a
  week/month", streaks and weekly progress; change a routine from a date on with history kept.
- Prerequisites with optional wait times; multi-step chores with variants (laundry: wash →
  dry → fold) that routines can start.
- Namedays and birthdays: a shared nameday calendar (the official Estonian list is downloaded,
  not bundled), personal people of interest, and lead-time tasks such as "buy a present"
  → "wish a happy birthday" (D-57).

### Household
- Groups: share projects, tasks, routines and goals; anyone can complete shared chores and it
  shows who did; live updates for everyone. Shared fixed-time routines appear on every
  member's timeline (D-54). Day plans, journal and tracking stay private.

### Calendar
- Read-only CalDAV (Nextcloud, Fastmail, iCloud, Radicale, …): events on the timeline and in
  a week agenda, busy time subtracted from free time, recurring events, exceptions and time
  zones handled. The password is stored encrypted (D-55, D-56).
- Tap an event to assign it (every instance of a recurring one) to a project and to attach
  todos to it; the project lists its upcoming events (D-68).

### Reflection and tracking
- Day summary with a month calendar; a private journal with prompts, also in the planner.
- Mood (several times a day), weight (kg/lb) and custom metrics with reminders; trend charts.
- Long-term goals with milestones, linked work, derived or manual progress, and weekly or
  monthly reviews (D-60).
- Export everything as JSON; CSV per metric; CSV import.

### App, notifications, operations
- Installable web app over HTTPS (Add to Home Screen), offline reading, and Web Push
  notifications that arrive with the app closed; optional ntfy (D-62).
- Daily database backups with retention, downloadable by admins (D-63).
- Multiple users, API tokens, a versioned JSON API with OpenAPI docs at `/api/docs`.
- One container image of about 13 MB that idles at about 7 MiB of RAM; amd64 and arm64.
