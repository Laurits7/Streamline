// The guided tour: a short walk through the app shown on first sign-in (D-77).
// Steps and card placement live here so they can be tested without a browser.

export type TourStep = {
  /** Page to show behind the step. */
  route: string
  /** Elements to highlight: the first one that is visible on screen wins. None: no highlight. */
  targets?: string[]
  title: string
  body: string
  /** Where to find the page when its nav link isn't on screen (phones). */
  where?: string
}

/** Nav link to a page: the sidebar on wide screens, the tab bar on phones. */
const nav = (href: string) => [`.sidebar a[href="${href}"]`, `.tabbar a[href="${href}"]`]

export const TOUR: TourStep[] = [
  {
    route: '/',
    title: 'Welcome to Streamline',
    body: 'A planner for you and your household. This short tour shows where things are; it takes about two minutes. You can leave at any time and replay it from Help.',
  },
  {
    route: '/',
    targets: ['[data-tour="quickadd"]'],
    title: 'Add tasks',
    body: 'Type a task and press Enter. Here it goes straight into today. Start typing a default task like “laundry” to get one with its checklist ready.',
  },
  {
    route: '/',
    targets: ['[data-tour="timeline"]'],
    title: 'The timeline',
    body: 'Tasks with a time, your calendar and time blocks. Drag a task onto it to give it a time; overlaps are pointed out with a fix.',
  },
  {
    route: '/',
    targets: ['[data-tour="plan"]'],
    title: 'Your plan for the day',
    body: 'Tasks you want to do today without a fixed time, in your order. “Pull in tasks” brings in work from your projects. The card at the top shows what’s up next; Focus starts a timer.',
  },
  {
    route: '/',
    targets: ['[data-tour="plan-day"]'],
    title: 'Plan the day',
    body: 'A guided planning ritual: look back at yesterday, choose what matters today and fit it into your time. You can get a reminder each morning or evening.',
  },
  {
    route: '/',
    targets: ['[data-tour="track"]'],
    title: 'Track how it went',
    body: 'Mood, health, habits and a journal line, a tap each. It feeds the summary and trends.',
  },
  {
    route: '/inbox',
    targets: nav('/inbox'),
    title: 'Inbox',
    body: 'Capture everything on your mind here without sorting it. Later, drag tasks onto a project or into a day.',
  },
  {
    route: '/projects',
    targets: nav('/projects'),
    title: 'Projects',
    body: 'Group tasks into projects (House, Work, Garden), with subprojects. Keep not-yet-started ones as ideas. Share a project with your household to plan it together.',
  },
  {
    route: '/routines',
    targets: nav('/routines'),
    title: 'Routines',
    where: 'On a phone: Projects → Routines.',
    body: 'Repeating chores and habits: daily, weekly or “some time this week”. Multi-step chores can wait for each other, and routines can have a checklist.',
  },
  {
    route: '/agenda',
    targets: nav('/agenda'),
    title: 'Agenda',
    body: 'The coming days at a glance, with your calendar (connect it in Settings), namedays and birthdays.',
  },
  {
    route: '/summary',
    targets: nav('/summary'),
    title: 'Reflect',
    where: 'On a phone: Settings → Summary & journal, Trends and Goals.',
    body: 'Summary & journal shows each day and month. Trends and Goals show how things go over time.',
  },
  {
    route: '/settings',
    targets: ['.sidebar a[href="/settings"]', '.tabbar a[href="/settings"]'],
    title: 'Settings',
    body: 'Your profile, groups for your household, notifications on your phone, calendars, places and default tasks.',
  },
  {
    route: '/',
    title: 'One more thing',
    body: 'Tap any task to open it: notes, a checklist, a repeat, a place, or “Wait for results” when it only needs checking later. Help explains everything, and has a button to take this tour again.',
  },
]

export type Rect = { top: number; left: number; width: number; height: number }

const GAP = 12
const MARGIN = 12

/**
 * Where to put the tour card (top-left corner) next to the highlighted element: below it if
 * it fits, otherwise above, otherwise over the bottom of the screen; kept inside the viewport.
 * Without a target the card is centred.
 */
export function placeCard(
  target: Rect | null,
  card: { width: number; height: number },
  view: { width: number; height: number },
): { top: number; left: number } {
  const maxLeft = Math.max(MARGIN, view.width - card.width - MARGIN)
  const maxTop = Math.max(MARGIN, view.height - card.height - MARGIN)
  if (!target) {
    return { top: Math.round(Math.min(maxTop, Math.max(MARGIN, (view.height - card.height) / 2))), left: Math.round(Math.min(maxLeft, Math.max(MARGIN, (view.width - card.width) / 2))) }
  }
  const left = Math.round(Math.min(maxLeft, Math.max(MARGIN, target.left + target.width / 2 - card.width / 2)))
  const below = target.top + target.height + GAP
  if (below + card.height + MARGIN <= view.height) return { top: Math.round(below), left }
  const above = target.top - GAP - card.height
  if (above >= MARGIN) return { top: Math.round(above), left }
  return { top: Math.round(maxTop), left }
}

/** The pref that records the tour was seen (finished or skipped), so it's offered only once. */
export const TOUR_PREF = 'tour'
