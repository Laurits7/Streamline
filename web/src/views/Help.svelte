<script lang="ts">
  // In-app guide to everything the app can do. Keep it in step with the features
  // (and with README.md when it describes the same thing).
  import Icon from '../lib/components/Icon.svelte'

  const sections = [
    ['start', 'Getting started'],
    ['today', 'The Today view'],
    ['planning', 'Planning your day'],
    ['dnd', 'Drag and drop'],
    ['views', 'List, board and matrix'],
    ['focus', 'Focus timer'],
    ['tasks', 'Tasks and their details'],
    ['dayend', 'What happens at the end of the day'],
    ['projects', 'Projects and the inbox'],
    ['devices', 'Phones, tablets and sync'],
    ['account', 'Account, users and API tokens'],
    ['soon', 'Coming later'],
  ]
</script>

<article>
  <h1>Help</h1>
  <p class="lead muted">How Streamline works and what you can do with it.</p>

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

  <section id="views">
    <h2><Icon name="columns" size={18} /> List, board and matrix</h2>
    <p>
      <a href="/tasks">All tasks</a>, the Inbox and every project can be shown in three ways (switch at the top; each
      place remembers its choice, on all your devices):
    </p>
    <ul>
      <li><strong>List</strong>: the normal ordered list.</li>
      <li>
        <strong>Board</strong>: columns by <em>status</em> (To do, In progress, Done), <em>project</em>
        (subprojects inside a project), <em>difficulty</em> or <em>task type</em>. Drag a card to another column to change
        that property, or within a column to reorder.
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
      <li>From the focus view: mark the task done (the next planned task starts), snooze it to tomorrow, or jump to the next task.</li>
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
      <li><strong>Notes</strong>: details, links, a checklist in plain text.</li>
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

  <section id="account">
    <h2><Icon name="settings" size={18} /> Account, users and API tokens</h2>
    <ul>
      <li><strong>Profile</strong>: your name, timezone, day end, the language used for dates, and which day your week starts on.</li>
      <li><strong>Password</strong>: changing it signs out your other devices.</li>
      <li><strong>Users</strong> (admins): add people from your household, reset passwords. There is no public sign-up.</li>
      <li><strong>API tokens</strong>: let scripts and other apps use Streamline (<code>Authorization: Bearer …</code> on <code>/api/v1</code>). The README lists the endpoints.</li>
    </ul>
  </section>

  <section id="soon">
    <h2>Coming later</h2>
    <p class="muted">Planned, roughly in this order:</p>
    <ul>
      <li>Repeating tasks and routines (daily, weekly, “twice a week”).</li>
      <li>Multi-step chores (e.g. laundry: wash → dry → fold) where the next step appears when the previous one is done.</li>
      <li>Shared household groups and chores anyone can complete.</li>
      <li>Your calendar (CalDAV/Radicale) on the timeline.</li>
      <li>Daily reflection, mood and other trackers, and long-term goals.</li>
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
