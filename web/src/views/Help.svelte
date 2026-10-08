<script lang="ts">
  // In-app guide to everything the app can do. Keep it in step with the features
  // (and with README.md when it describes the same thing).
  import Icon from '../lib/components/Icon.svelte'
  import { ui } from '../lib/ui.svelte'

  const sections = [
    ['start', 'Getting started'],
    ['today', 'The Today view'],
    ['planning', 'Planning your day'],
    ['blocks', 'Time blocks, overlaps and suggested times'],
    ['dnd', 'Drag and drop'],
    ['routines', 'Routines and repeating tasks'],
    ['steps', 'Prerequisites and multi-step chores'],
    ['views', 'List, board and matrix'],
    ['focus', 'Focus timer'],
    ['tasks', 'Tasks and their details'],
    ['dayend', 'What happens at the end of the day'],
    ['projects', 'Projects and the inbox'],
    ['devices', 'Phones, tablets and sync'],
    ['install', 'Install on your phone, notifications'],
    ['groups', 'Sharing with your household'],
    ['places', 'Places'],
    ['calendar', 'Your calendar and the agenda'],
    ['occasions', 'Namedays and birthdays'],
    ['tracking', 'Summary, journal and tracking'],
    ['goals', 'Long-term goals'],
    ['account', 'Account, users and API tokens'],
    ['soon', 'Coming later'],
  ]
</script>

<article>
  <h1>Help</h1>
  <p class="lead muted">How Streamline works and what you can do with it.</p>
  <button class="btn primary tour-btn" onclick={() => (ui.tour = 0)}><Icon name="target" size={16} /> Take the tour</button>

  <nav class="toc card" aria-label="Contents">
    {#each sections as [id, title] (id)}
      <a href="#{id}" onclick={(e) => { e.preventDefault(); document.getElementById(id)?.scrollIntoView({ behavior: 'smooth' }) }}>{title}</a>
    {/each}
  </nav>

  <section id="start">
    <h2>Getting started</h2>
    <ol>
      <li>Capture everything on your mind in the <a href="/inbox">Inbox</a>. Type and press Enter; don't worry about sorting yet.</li>
      <li>Create a few <a href="/projects">projects</a> (e.g. “House”, “Work”, “Garden”) and drag inbox tasks onto them.</li>
      <li>Each day, open <a href="/">Today</a>, pull in what you want to do, put fixed things on the timeline, and tick tasks off.</li>
    </ol>
    <p>New here? <strong>Take the tour</strong> (the button above, or in Settings) for a two-minute walk through where everything is. It's offered once when you first sign in.</p>
    <p>Tasks live in projects. Planning a day doesn't move them: the day plan is a separate layer on top, so your projects stay tidy.</p>
  </section>

  <section id="today">
    <h2><Icon name="sun" size={18} /> The Today view</h2>
    <p>This is the start page. It shows what to expect today:</p>
    <ul>
      <li><strong>Up next</strong>: the next scheduled task, or the first one in your plan.</li>
      <li><strong>Timeline</strong>: tasks with a time, with a red line for “now”.</li>
      <li><strong>Plan</strong>: tasks you've pulled into the day without a fixed time, in your order.</li>
      <li><strong>Due &amp; overdue</strong>: open tasks whose due date has come, which aren't planned yet. Tap the sun to add one.</li>
      <li><strong>Done &amp; closed</strong>: what you finished (or missed) today.</li>
    </ul>
    <p>
      <strong>Pull in tasks</strong> opens your ready stack: every open task, grouped by project, with a search box. Tap
      a task to add it to the day. Typing in “Add a task for today…” creates a new inbox task that is already planned
      for today.
    </p>
    <p>Use the arrows next to the date to look at, and plan, other days.</p>
    <p>
      <strong>Activity log</strong>: each day keeps an automatic log of what happened — tasks completed, started,
      added, skipped or missed, focus time and planning — in time order. It's on the day view and in the evening
      review, and <em>Copy as text</em> gives you a summary to reflect on or paste elsewhere.
    </p>
  </section>

  <section id="planning">
    <h2><Icon name="calendar" size={18} /> Planning your day</h2>
    <p>
      A few minutes of planning makes a day calmer. Streamline guides you through it in five short steps, either in
      the evening for tomorrow or in the morning for today (choose in <a href="/settings">Settings</a>):
    </p>
    <ol>
      <li><strong>Review</strong>: what got done, what was missed and what's still open. Finish, move, unplan or drop the open ones.</li>
      <li><strong>Look ahead</strong>: what's already scheduled for the day and what's due.</li>
      <li><strong>Pick</strong>: add tasks from your projects. The bar compares planned time (from estimates) with your free time and warns when the day is overbooked.</li>
      <li><strong>Arrange</strong>: drag to set the order, and drop tasks on the timeline to give them a time.</li>
      <li><strong>Confirm</strong>: mark the day as planned.</li>
    </ol>
    <p>
      At your planning time you get one reminder, and Today shows a banner until the day is planned (you can dismiss
      it). If you stop halfway, the planner picks up where you left off, on any device. Planned days show
      <em>✓ Planned</em> next to the date; tap it to adjust. Skipping planning is fine: Today still shows everything
      that's scheduled, due or carried over.
    </p>
    <p class="muted">“Free time” is the part of the day you set in Settings (e.g. 08:00–22:00) minus scheduled tasks.</p>
  </section>

  <section id="dnd">
    <h2><Icon name="grip" size={18} /> Drag and drop</h2>
    <dl>
      <dt>With a mouse</dt>
      <dd>Press on a task and move it.</dd>
      <dt>On a phone or tablet</dt>
      <dd>Press and hold a task for a moment, then move it (a short buzz tells you it's picked up). Or drag the grip <Icon name="grip" size={14} /> on the left straight away. Swiping normally just scrolls.</dd>
      <dt>With a keyboard</dt>
      <dd>Tab to a task's grip and use <kbd>↑</kbd> / <kbd>↓</kbd> to move it in the list.</dd>
    </dl>
    <p>Where you can drop things:</p>
    <ul>
      <li><strong>Within a list</strong>: reorder tasks or projects.</li>
      <li><strong>On a project, Inbox or Today</strong> in the sidebar (on a phone: the Today and Inbox tabs, or the rows on the Projects page): moves the task to that project, or plans it for today.</li>
      <li><strong>On the timeline</strong>: gives the task a time (in 15-minute steps). Drag a block to move it, drag its bottom edge to make it longer or shorter, or drag it back into the Plan list to remove the time.</li>
    </ul>
    <p class="muted">Everything you can drag can also be done from the task's details, for small screens and keyboards.</p>
  </section>

  <section id="routines">
    <h2><Icon name="repeat" size={18} /> Routines and repeating tasks</h2>
    <p>Create them on the <a href="/routines">Routines</a> page. There are three kinds:</p>
    <ul>
      <li><strong>On a schedule</strong>: due on certain days, e.g. pay rent on the 1st, bins out every Tuesday. Daily, weekdays, certain days, every N days, monthly (also “last day”), yearly, or any iCalendar rule.</li>
      <li><strong>At a fixed time</strong>: e.g. vitamins every morning at 8:00. It appears on the day's timeline by itself.</li>
      <li><strong>N times a week or month</strong>: e.g. laundry twice a week, any day you like. Today shows them under “This week / month” with your progress (1/2).</li>
    </ul>
    <p>
      Each occurrence is an ordinary task: plan it, focus on it, <strong>move it to another day</strong> when something
      clashes (it keeps its time), or <strong>Skip this time</strong>. The Routines page shows how this week is going,
      e.g. “3 of 4 this week” — skipped or deleted ones count as not done. Editing
      that task changes only that occurrence; <strong>Edit routine</strong> changes it from a date on, and earlier
      occurrences keep their history. Occurrences appear the day before (so you can plan them in the evening).
    </p>
    <p>
      What happens when one isn't done depends on its type: fixed-time routines <em>expire</em> (counted as missed),
      “N times” routines are missed if the week or month ends first, and scheduled ones <em>carry on</em> by default.
      The Routines page shows your streak 🔥 for each.
    </p>
  </section>

  <section id="steps">
    <h2><Icon name="list" size={18} /> Prerequisites and multi-step chores</h2>
    <p>
      A task can <strong>wait for</strong> other tasks: open it and choose <em>Must be done after…</em>. Until those are
      done it shows <em>⏳ Waiting on …</em>, stays out of the ready stack and the planner, and isn't counted as missed.
      The moment the last one is done it becomes ready, on every device. Optionally it waits a bit longer first
      (<em>then wait 1h</em>, while a machine runs) and tells you when it's ready. If a prerequisite is skipped or
      deleted, the waiting task is freed.
    </p>
    <p>
      <strong>Waiting for results.</strong> Some work is done but you can't finish it yet: a model is training, a build
      runs, you've sent an e-mail and need the reply. Open the task and choose <em>Wait for results…</em> (or the same
      button in the focus view), say what you're waiting for and when to check back: in 30 minutes, 1, 2 or 4 hours,
      tomorrow morning, at a time you pick, or no time. While it waits it needs nothing from you: it leaves Up next and
      the ready stack, takes no time on the timeline (a dashed line marks the check-back time), isn't counted as
      missed and carries on to the next day. Today lists it under <em>Waiting for results</em>, and the board has a
      <em>Waiting</em> column. At the check-back time you get a reminder and the task comes back at the top of Up next
      with <em>Check back</em>: mark it done, wait more, or go back to work on it. Without a time it waits until you
      change it.
    </p>
    <p>
      <strong>Multi-step chores</strong> (on the <a href="/routines">Routines</a> page) do this for you. Laundry is a
      chain: wash → dry → iron → fold, with variants that leave steps out (delicates skip the dryer, towels skip
      ironing). <em>Start</em> it, pick the loads, and each load gets its own chain; only the current step of each shows
      up in your lists. The dots on a step show how far along the chain is.
    </p>
  </section>

  <section id="views">
    <h2><Icon name="columns" size={18} /> List, board and matrix</h2>
    <p>
      <a href="/tasks">All tasks</a>, the Inbox and every project can be shown in three ways (switch at the top; each
      place remembers its choice, on all your devices):
    </p>
    <ul>
      <li><strong>List</strong>: the normal ordered list.</li>
      <li>
        <strong>Board</strong>: columns by <em>status</em> (To do, In progress, Waiting, Done), <em>project</em>
        (subprojects inside a project), <em>difficulty</em> or <em>task type</em>. Drag a card to another column to change
        that property, or within a column to reorder. Marked something done by mistake? Drag it from <em>Done</em>
        back to <em>To do</em>.
      </li>
      <li>
        <strong>Matrix</strong>: urgent/important quadrants (Do first, Schedule, Delegate, Later). Dragging a task to
        another quadrant sets its urgency and importance, keeping finer values (like “very important”) when only the
        other axis changes.
      </li>
    </ul>
    <p class="muted">These are just views: the tasks stay where they are, with the same order and details.</p>
  </section>

  <section id="focus">
    <h2><Icon name="target" size={18} /> Focus timer</h2>
    <p>
      Open a task and choose <strong>Focus on this</strong> (or <strong>Focus</strong> on “Up next”). The focus view
      shows just that task with a Pomodoro timer: 25 minutes of focus, then a 5-minute break, and a longer break after
      every 4 rounds (change the lengths in <a href="/settings">Settings</a>).
    </p>
    <ul>
      <li>Pause, resume, skip or stop at any time. A chime and a buzz tell you when an interval ends.</li>
      <li>The timer runs on the server: start it on your phone and the same countdown shows on your computer. Leave the focus view and a small timer stays in the corner.</li>
      <li>Focus time is added to the task's <em>time spent</em>, so you can compare it with your estimate. Starting focus also marks the task <em>in progress</em>.</li>
      <li>From the focus view: mark the task done (the next planned task starts), set it <em>waiting for results</em> and move on, snooze it to tomorrow, or jump to the next task.</li>
    </ul>
  </section>

  <section id="tasks">
    <h2><Icon name="edit" size={18} /> Tasks and their details</h2>
    <p>Tap a task to open its details. Changes save immediately.</p>
    <ul>
      <li><strong>Day plan</strong>: plan it for today, tomorrow or any date, set a time and a duration, or unplan it.</li>
      <li><strong>Project</strong> and <strong>due date</strong>.</li>
      <li><strong>Estimate</strong>: how long you think it takes. Used for “time left” on the Today view and as the default block length on the timeline.</li>
      <li><strong>Difficulty</strong> (easy, medium, hard), <strong>importance</strong> and <strong>urgency</strong>. Important and urgent tasks get a label in lists. Later these will drive the planner and the priority matrix.</li>
      <li><strong>Checklist</strong>: the steps inside a task (Do laundry: sort, wash, hang, fold). Tick them as you go; lists, the timeline and the board show how far along it is (2/4). Ticking the last step offers to complete the task. A task carried to the next day keeps its ticks. Routines can have a checklist too: each occurrence starts unticked.</li>
      <li><strong>Notes</strong>: details and links.</li>
      <li><strong>Save as default task</strong> (at the bottom): keep a ready-made copy (title, checklist, estimate and the rest) of a task you add often. Then start typing in any “Add a task…” field (<em>laun…</em>) and pick <em>Do laundry</em>, or tap the list button at the end of the field. Its checklist starts unticked. Manage them in <a href="/settings#default-tasks">Settings → Default tasks</a>, where you can also share them with a group.</li>
      <li><strong>Won't do</strong> closes a task without completing it. <strong>Delete</strong> removes it.</li>
    </ul>
    <p>Completing a task shows an <em>Undo</em> button for a few seconds.</p>
  </section>

  <section id="dayend">
    <h2><Icon name="repeat" size={18} /> What happens at the end of the day</h2>
    <p>Each task has a type that decides what happens if it's planned for a day but not done:</p>
    <ul>
      <li><strong>Carry on</strong> (default): it moves to the next day and shows how many days it has been carried over (e.g. <Icon name="repeat" size={12} /> 2d).</li>
      <li><strong>Expires</strong>: for things that only make sense that day (take vitamins, water the plants). It's marked <em>missed</em> and doesn't follow you around.</li>
    </ul>
    <p>
      Your day ends at <strong>04:00</strong> by default, so a late evening still counts as the same day. You can change
      this and your timezone in <a href="/settings">Settings</a>.
    </p>
    <p>To push a task to later yourself, open it and plan it for another day.</p>
  </section>

  <section id="projects">
    <h2><Icon name="folder" size={18} /> Projects and the inbox</h2>
    <p>
      The <strong>Inbox</strong> holds tasks that don't belong to a project yet. It's the place to capture things quickly
      and sort them later: drag them onto a project, plan them for a day, or delete them.
    </p>
    <p>
      <strong>Projects</strong> group related tasks. From a project's <Icon name="more" size={14} /> menu you can
      rename it, pick a colour, move it under another project, archive it (hides it but keeps everything) or delete it
      together with its tasks. The sun button on a task plans it for today.
    </p>
    <p>
      A task can be listed in <strong>several projects</strong>: open it and use <em>Also in → + Add project</em>
      (e.g. “buy a garden hose” for both <em>Cottage</em> and <em>Shopping</em>). Its main project is where it was
      created; deleting another project only removes the link.
    </p>
    <p>
      <strong>Subprojects</strong> break bigger things down, as deep as you like. For example, “Paper” can contain
      “Research”, “Analysis” and “Writing”, and “Writing” can contain “Draft 1”. Add one with “Add a subproject…” at
      the bottom of a project, or drag a project onto the middle of another project on the Projects page. Dragging near
      a project's top or bottom edge reorders instead.
    </p>
    <p>
      To turn a subproject into a full project of its own, choose <strong>Make top-level project</strong> in its
      <Icon name="more" size={14} /> menu, or drag it between the top-level projects on the Projects page. Its tasks
      and subprojects come with it.
    </p>
    <p>
      A project's page shows its own tasks and a section for each subproject; drag tasks between the sections. Counts
      on the Projects page include subprojects. Archiving or deleting a project also archives or deletes its
      subprojects.
    </p>
    <p>
      Every project gets its own <strong>colour and shape</strong> (a red circle, a blue square, a green triangle…) so
      you can spot it in lists; subprojects keep their parent's colour with a shape of their own. Change them in the
      project's <Icon name="more" size={14} /> menu.
    </p>
    <p>
      Every project has a <strong>description</strong> at the top of its page: what it's about, goals, notes and
      links (web addresses become clickable). Use <em>Add a description</em> or <em>Edit description</em>.
    </p>
    <p>
      Not everything is under way yet. Tick <strong>Just an idea for now</strong> when you add a project, or choose
      <em>Move to ideas</em> in its <Icon name="more" size={14} /> menu. An <span class="idea-tag">Idea</span> stays in
      the Projects list with a grey tag and an empty grey circle, and you can add a description, tasks and subprojects to it, but nothing in it
      asks for attention: its tasks stay out of Today, the planning wizard, All tasks and reminders, can't be planned,
      and are never missed or carried over. Its routines don't run. When you're ready, press
      <strong>Activate</strong> on its page and everything in it works normally. Moving a project to ideas takes its
      open tasks off your plan from today on. An idea can also sit inside an active project: tick
      <em>Just an idea for now</em> under “Add a subproject…”.
    </p>
  </section>

  <section id="devices">
    <h2>Phones, tablets and sync</h2>
    <p>
      Open the same address on any device on your network and sign in. Changes appear on your other devices
      instantly. The dot next to “Streamline” in the sidebar is green while live sync is connected.
    </p>
    <p>
      If you're offline, a change you make is rejected with a message and undone. Full offline use is planned for
      later.
    </p>
  </section>

  <section id="groups">
    <h2><Icon name="users" size={18} /> Sharing with your household</h2>
    <p>
      Create a group in <a href="/settings">Settings</a> (e.g. “Family”) and add people. Then share a project with it
      (project <Icon name="more" size={14} /> menu → <em>Shared with</em>): its subprojects, tasks and routines are shared
      too. Tasks and routines without a project can be shared on their own.
    </p>
    <ul>
      <li>Everyone in the group sees shared things, and <strong>anyone can complete them</strong> — it shows who did (“✓ Anna”).</li>
      <li>Changes appear instantly for everyone: when someone takes out the garbage, it disappears from your list too.</li>
      <li>Repeating chores (bins out every Tuesday) have one shared occurrence, not one each.</li>
      <li>A shared routine with a fixed time (walk the dog at 7:30) lands on every member's timeline at that time. Whoever does it ticks it off, and it leaves everyone else's day at once.</li>
      <li>Each person plans their own day: you can put a shared chore on your plan without it landing on anyone else's.</li>
      <li>Your day plans, focus time, activity log and notes are never shared.</li>
    </ul>
  </section>

  <section id="places">
    <h2><Icon name="pin" size={18} /> Places</h2>
    <p>
      Give tasks a place where they have to be done (Home, Cottage, Town, Office…). Create places in
      <a href="/settings">Settings</a>; a project can give new tasks a default place, and routines a place for each
      occurrence. A task's place can differ from its project's: “buy a garden hose” for the cottage is done in town.
    </p>
    <p>
      Choose where you are in <a href="/settings">Settings</a> (<em>I'm currently at</em>). The ready stack, planner,
      All tasks, board and matrix then show only what can be done there, plus tasks without a place. Project pages and
      your day plan always show everything.
    </p>
    <p>
      Prefer it automatic? Save a location for your places and turn on <em>Detect my place by GPS</em> in Settings.
      Browsers only allow this over HTTPS; on a plain <code>http://</code> address you pick the place by hand.
    </p>
  </section>

  <section id="account">
    <h2><Icon name="settings" size={18} /> Account, users and API tokens</h2>
    <ul>
      <li><strong>Profile</strong>: your name, timezone, day end, the language used for dates, and which day your week starts on.</li>
      <li><strong>Password</strong>: changing it signs out your other devices.</li>
      <li><strong>Users</strong> (admins): add people from your household, reset passwords. There is no public sign-up.</li>
      <li><strong>API tokens</strong>: let scripts and other apps use Streamline (<code>Authorization: Bearer …</code> on <code>/api/v1</code>). The README lists the endpoints.</li>
    </ul>
  </section>

  <section id="blocks">
    <h2><Icon name="zap" size={18} /> Time blocks, overlaps and suggested times</h2>
    <p>
      Split your days into <strong>time blocks</strong> with a theme: “Deep work” in the morning for hard tasks, “Admin” after
      lunch for easy ones. Make <strong>day templates</strong> in <a href="/settings">Settings</a> and give each the weekdays
      it's for; the coming week's days get their blocks automatically. On the timeline, drag a block's label to move it,
      drag the little bar at its bottom right to resize it, or click the label to edit it. The <em>Blocks…</em> menu
      above the timeline switches a day to another template, adds a block, or removes them all. A day you've changed by
      hand keeps your changes.
    </p>
    <ul>
      <li><strong>Overlaps:</strong> when a scheduled task overlaps a calendar event or another task, it's outlined in red and a banner offers <em>Move to free time</em>, <em>Shorten</em> or <em>Unschedule</em>. If a calendar change causes an overlap today or tomorrow, you get a notification.</li>
      <li><strong>Suggest times:</strong> in the planner's <em>Arrange</em> step, Streamline proposes times for the tasks you picked: overdue and due-soon first, then urgent and important; hard tasks in “hard” blocks, easy ones in “easy” blocks; always in free time. Each suggestion says why. Untick what you don't want and press <em>Use selected</em>.</li>
      <li>All-day events don't block time unless you tick <em>all-day = busy</em> for that calendar in Settings.</li>
    </ul>
  </section>

  <section id="calendar">
    <h2><Icon name="calendar" size={18} /> Your calendar and the agenda</h2>
    <p>
      Connect your calendar in <a href="/settings">Settings</a> → <em>Calendar</em> with its CalDAV address, username
      and password: the same details a calendar app like Evolution or Thunderbird asks for. It works with Nextcloud,
      Fastmail, iCloud (use an app-specific password), mailbox.org, Radicale and other CalDAV servers. Google Calendar
      needs a sign-in flow that isn't supported yet.
    </p>
    <ul>
      <li>Events show on the day's timeline in their calendar's colour, next to your scheduled tasks. All-day events appear above it.</li>
      <li>Busy events count against the day's free time in the planner; events marked “free” are striped and don't.</li>
      <li><a href="/agenda">Agenda</a> shows a week at a time: events and timed tasks, day by day.</li>
      <li><strong>Tap an event</strong> to put it in a project (for a repeating event, every time it repeats) and to add todos for it, e.g. “Prepare the agenda” for a meeting. Todos are due on the event's day and go into its project; the event shows how many are done (✓ 1/2), the project page lists its upcoming events, and the todo links back to its event.</li>
      <li>Streamline only reads your calendar. It checks for changes every 15 minutes, or press <em>Sync now</em>.</li>
      <li>Untick a calendar to hide it, or pick another colour. If the server can't be reached, the last known events stay and Settings shows the error.</li>
      <li>Your password is stored encrypted on the server and never sent back to the browser. Other people never see your events: a todo for one of your events in a shared project is visible to the group, but not the event it's for.</li>
    </ul>
  </section>

  <section id="tracking">
    <h2><Icon name="book" size={18} /> Summary, journal and tracking</h2>
    <p>
      The <em>Track</em> card on the Today view logs your mood (tap a face whenever you think of it: each tap is a new entry with its time, and the day shows the average and how it went through the day), weight, and anything you add yourself in
      <a href="/settings">Settings</a> (sleep, water, steps, “took vitamins”…). Log as often as you like; each metric
      decides how a day's values combine (latest, average, sum or highest).
    </p>
    <ul>
      <li><strong>Health</strong>: once a day, tap how you are: feeling great, OK, not feeling well, sick or injured. Sick asks what it is (cold, flu, fever, stomach, headache, other) and injured asks where (back, neck, hand, arm, knee, foot, other); add a note if you like. It's on the Today view's <em>Track</em> card, in the planner's reflection step and on the summary (where you can fill in past days). Sick and injured days are marked on the month calendar, and <a href="/trends">Trends</a> shows a strip of your days. It's only a record: it doesn't change streaks or plans, and nobody else sees it.</li>
      <li><a href="/summary">Summary &amp; journal</a> shows a day at a glance: planned vs. done, what was carried over or missed, routines and streaks, events, focus time and what you tracked. Write your reflection there (or in the planner's first step); it saves as you type. The month calendar lets you browse past days.</li>
      <li><a href="/trends">Trends</a> charts each metric over a week, month or year, and your mood next to how many routines you did.</li>
      <li>Weight is shown in kg or lb (Settings → Tracking → Units). A reminder time per metric nudges you if you haven't logged it by then.</li>
      <li>Export: CSV per metric, or all your data as JSON. Import past values from a CSV of <code>date,value</code>.</li>
      <li>Your journal and tracking are private: nobody else sees them, not even people in your groups.</li>
    </ul>
  </section>

  <section id="goals">
    <h2><Icon name="flag" size={18} /> Long-term goals</h2>
    <p>
      <a href="/goals">Goals</a> are the bigger things you're working towards (“Run a half marathon”, “Finish the
      renovation”). Give one a target date, break it into milestones, and link the projects and tasks that move it
      forward: progress fills up as milestones and linked tasks get done (tasks in subprojects count too). You can also
      set the progress by hand.
    </p>
    <ul>
      <li>Once a week (or month; choose on the Goals page) the Today view invites you to a short <strong>review</strong>: where each goal stands, a note on each, and your reflection. Notes are kept with the goal.</li>
      <li>Goals are yours unless you share one with a group, like a project.</li>
    </ul>
  </section>

  <section id="occasions">
    <h2><Icon name="gift" size={18} /> Namedays and birthdays</h2>
    <p>
      <a href="/occasions">Namedays &amp; birthdays</a> holds the nameday calendar (shared by everyone on this server) and
      your own list of people. Search for a name (accents are optional: “tonu” finds Tõnu) and press <em>Add</em>, or add
      someone with just a birthday.
    </p>
    <ul>
      <li>A week ahead, tasks appear in your inbox, each due on its day. By default, birthdays get “Buy a present” two days before and “Wish them a happy birthday” on the day (which waits until the present is bought); namedays get a greeting on the day.</li>
      <li>Change the steps under <em>What gets added</em>: titles (<code>{'{name}'}</code> becomes the person's name), how many days before, the task type, and whether a step waits for the previous one.</li>
      <li>The day view and the agenda show each day's namedays, with your people highlighted.</li>
      <li>Admins can re-download the official Estonian list or upload another country's list.</li>
    </ul>
  </section>

  <section id="install">
    <h2><Icon name="bolt" size={18} /> Install on your phone, notifications</h2>
    <p>
      When Streamline is opened over HTTPS you can put it on your home screen: on an <strong>iPhone</strong>, open it in
      Safari and tap <em>Share → Add to Home Screen</em>; on Android, use the browser's <em>Install app</em>. It then opens
      full screen like any app, and shows the data it last loaded when you're offline.
    </p>
    <ul>
      <li><strong>Notifications</strong>: in <a href="/settings">Settings → Notifications</a>, tap <em>Turn on</em> on each device (on iPhone, from the home-screen app). You then get planning reminders, overlap warnings and the like even when Streamline is closed. Choose which kinds there too.</li>
      <li>Without HTTPS, everything else works in the browser as usual; the <a href="https://ntfy.sh">ntfy</a> app can still deliver notifications (Settings → Notifications).</li>
    </ul>
  </section>

  <section id="soon">
    <h2>Coming later</h2>
    <p class="muted">Planned, roughly in this order:</p>
    <ul>
      <li>A shopping-list and meal-plan link with Mealie, and printable day sheets.</li>
      <li>Writing planned time blocks back to your calendar.</li>
    </ul>
  </section>
</article>

<style>
  article {
    max-width: 680px;
  }
  h1 {
    font-size: 28px;
    font-weight: 750;
    letter-spacing: -0.02em;
  }
  .lead {
    margin: 4px 0 16px;
  }
  .tour-btn {
    margin-bottom: 14px;
  }
  .toc {
    display: flex;
    flex-direction: column;
    padding: 8px;
    margin-bottom: 8px;
  }
  .toc a {
    padding: 8px 10px;
    border-radius: 8px;
    color: var(--accent);
    font-weight: 550;
  }
  .toc a:hover {
    background: var(--surface-2);
  }
  section {
    padding-top: 24px;
    scroll-margin-top: 16px;
  }
  h2 {
    font-size: 19px;
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 8px;
  }
  p,
  li,
  dd {
    line-height: 1.55;
  }
  ul,
  ol {
    padding-left: 20px;
  }
  li {
    margin: 4px 0;
  }
  li :global(svg),
  dd :global(svg),
  p :global(svg) {
    vertical-align: -2px;
  }
  dt {
    font-weight: 650;
    margin-top: 10px;
  }
  dd {
    margin: 2px 0 0;
  }
  a {
    color: var(--accent);
  }
  kbd,
  code {
    font-size: 12px;
    background: var(--surface-2);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 1px 5px;
  }
</style>
