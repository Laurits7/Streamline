//! Pomodoro focus timer (SPEC §6.9) as a pure state machine. The server keeps one
//! `Timer` per user; any device can show the countdown from it. Times are Unix
//! milliseconds.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Work,
    ShortBreak,
    LongBreak,
}

impl Phase {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "idle" => Self::Idle,
            "work" => Self::Work,
            "short_break" => Self::ShortBreak,
            "long_break" => Self::LongBreak,
            _ => return None,
        })
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Work => "work",
            Self::ShortBreak => "short_break",
            Self::LongBreak => "long_break",
        }
    }
    pub fn is_break(self) -> bool {
        matches!(self, Self::ShortBreak | Self::LongBreak)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settings {
    pub work_min: u32,
    pub short_break_min: u32,
    pub long_break_min: u32,
    /// A long break follows every N-th completed work interval.
    pub long_every: u32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            work_min: 25,
            short_break_min: 5,
            long_break_min: 15,
            long_every: 4,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timer {
    pub phase: Phase,
    pub task_id: Option<String>,
    /// Set while the clock runs; `None` = paused (or idle, or waiting to start).
    pub running_since: Option<i64>,
    /// Time accumulated before `running_since`.
    pub elapsed_ms: i64,
    pub length_min: u32,
    /// Completed work intervals since the last long break.
    pub cycle_done: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionKind {
    Work,
    Break,
}

/// A finished interval to log.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session {
    pub kind: SessionKind,
    pub task_id: Option<String>,
    pub started_ms: i64,
    pub ended_ms: i64,
    pub minutes: u32,
    /// Ran its full length (vs skipped/stopped early).
    pub completed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Start working on a task (or without one). Interrupts whatever is running.
    Start {
        task_id: Option<String>,
    },
    Pause,
    Resume,
    /// End the current interval early and move to the next.
    Skip,
    /// End focusing altogether.
    Stop,
}

const MIN: i64 = 60_000;

impl Timer {
    pub fn idle() -> Self {
        Self {
            phase: Phase::Idle,
            task_id: None,
            running_since: None,
            elapsed_ms: 0,
            length_min: 0,
            cycle_done: 0,
        }
    }

    pub fn elapsed(&self, now: i64) -> i64 {
        self.elapsed_ms + self.running_since.map_or(0, |s| (now - s).max(0))
    }

    pub fn remaining(&self, now: i64) -> i64 {
        (i64::from(self.length_min) * MIN - self.elapsed(now)).max(0)
    }

    fn session(&self, end: i64, completed: bool) -> Option<Session> {
        if self.phase == Phase::Idle {
            return None;
        }
        let elapsed = self.elapsed(end);
        let minutes = ((elapsed + MIN / 2) / MIN) as u32;
        // Intervals abandoned within the first minute aren't worth logging.
        if !completed && elapsed < MIN {
            return None;
        }
        Some(Session {
            kind: if self.phase == Phase::Work {
                SessionKind::Work
            } else {
                SessionKind::Break
            },
            task_id: self.task_id.clone(),
            started_ms: end - elapsed,
            ended_ms: end,
            minutes,
            completed,
        })
    }

    fn begin(&mut self, phase: Phase, at: Option<i64>, s: &Settings) {
        self.phase = phase;
        self.running_since = at;
        self.elapsed_ms = 0;
        self.length_min = match phase {
            Phase::Idle => 0,
            Phase::Work => s.work_min,
            Phase::ShortBreak => s.short_break_min,
            Phase::LongBreak => s.long_break_min,
        };
    }

    fn break_after_work(&self, s: &Settings) -> Phase {
        if self.cycle_done > 0 && self.cycle_done.is_multiple_of(s.long_every.max(1)) {
            Phase::LongBreak
        } else {
            Phase::ShortBreak
        }
    }

    /// Complete every interval whose time has run out by `now` (possibly several, if
    /// nobody looked for a while). Work rolls into a break automatically; after a break
    /// the next work interval waits for the user to start it.
    pub fn advance(mut self, now: i64, s: &Settings) -> (Timer, Vec<Session>) {
        let mut out = Vec::new();
        while let Some(since) = self.running_since {
            if self.phase == Phase::Idle || self.elapsed(now) < i64::from(self.length_min) * MIN {
                break;
            }
            let end = since + i64::from(self.length_min) * MIN - self.elapsed_ms;
            out.extend(self.session(end, true));
            if self.phase == Phase::Work {
                self.cycle_done += 1;
                let next = self.break_after_work(s);
                self.begin(next, Some(end), s);
            } else {
                if self.phase == Phase::LongBreak {
                    self.cycle_done = 0;
                }
                self.begin(Phase::Work, None, s);
            }
        }
        (self, out)
    }

    /// Apply a user action at `now` (after completing anything already due).
    pub fn apply(self, action: Action, now: i64, s: &Settings) -> (Timer, Vec<Session>) {
        let (mut t, mut out) = self.advance(now, s);
        match action {
            Action::Start { task_id } => {
                out.extend(t.session(now, false));
                if t.phase == Phase::LongBreak {
                    t.cycle_done = 0; // cutting a long break short still starts a new cycle
                }
                t.task_id = task_id;
                t.begin(Phase::Work, Some(now), s);
            }
            Action::Pause => {
                if t.running_since.is_some() {
                    t.elapsed_ms = t.elapsed(now);
                    t.running_since = None;
                }
            }
            Action::Resume => {
                if t.running_since.is_none() && t.phase != Phase::Idle {
                    t.running_since = Some(now);
                }
            }
            Action::Skip => match t.phase {
                Phase::Idle => {}
                Phase::Work => {
                    out.extend(t.session(now, false));
                    let next = t.break_after_work(s);
                    t.begin(next, Some(now), s);
                }
                Phase::ShortBreak | Phase::LongBreak => {
                    out.extend(t.session(now, false));
                    if t.phase == Phase::LongBreak {
                        t.cycle_done = 0;
                    }
                    t.begin(Phase::Work, None, s);
                }
            },
            Action::Stop => {
                out.extend(t.session(now, false));
                t = Timer::idle();
            }
        }
        (t, out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const S: Settings = Settings {
        work_min: 25,
        short_break_min: 5,
        long_break_min: 15,
        long_every: 4,
    };
    const T0: i64 = 1_800_000_000_000;
    fn m(n: i64) -> i64 {
        T0 + n * MIN
    }
    fn start() -> Timer {
        Timer::idle()
            .apply(
                Action::Start {
                    task_id: Some("task".into()),
                },
                T0,
                &S,
            )
            .0
    }

    #[test]
    fn work_rolls_into_a_short_break_and_logs_a_session() {
        let t = start();
        assert_eq!(t.remaining(m(10)), 15 * MIN);
        let (t, sessions) = t.advance(m(26), &S);
        assert_eq!(t.phase, Phase::ShortBreak);
        assert_eq!(
            t.running_since,
            Some(m(25)),
            "the break started when work ended, not when we looked"
        );
        assert_eq!(t.remaining(m(26)), 4 * MIN);
        assert_eq!(
            sessions,
            [Session {
                kind: SessionKind::Work,
                task_id: Some("task".into()),
                started_ms: T0,
                ended_ms: m(25),
                minutes: 25,
                completed: true
            }]
        );
    }

    #[test]
    fn after_a_break_the_next_work_interval_waits() {
        let (t, sessions) = start().advance(m(31), &S);
        assert_eq!(sessions.len(), 2);
        assert_eq!(sessions[1].kind, SessionKind::Break);
        assert_eq!(
            (t.phase, t.running_since, t.task_id.as_deref()),
            (Phase::Work, None, Some("task"))
        );
        // ...and nothing more happens however long we wait.
        assert_eq!(t.clone().advance(m(500), &S).0, t);
    }

    #[test]
    fn every_fourth_work_interval_earns_a_long_break() {
        let mut t = start();
        let mut now = T0;
        for i in 1..=4 {
            now += 25 * MIN;
            t = t.advance(now, &S).0;
            let expect = if i == 4 {
                Phase::LongBreak
            } else {
                Phase::ShortBreak
            };
            assert_eq!(t.phase, expect, "after work #{i}");
            now += i64::from(t.length_min) * MIN;
            t = t.advance(now, &S).0.apply(Action::Resume, now, &S).0;
        }
        assert_eq!(t.cycle_done, 0, "the cycle restarts after the long break");
    }

    #[test]
    fn pausing_stops_the_clock() {
        let t = start().apply(Action::Pause, m(10), &S).0;
        assert_eq!(t.remaining(m(100)), 15 * MIN);
        let t = t.apply(Action::Resume, m(100), &S).0;
        assert_eq!(t.remaining(m(105)), 10 * MIN);
        let (t, s) = t.advance(m(115), &S);
        assert_eq!(t.phase, Phase::ShortBreak);
        assert_eq!(s[0].ended_ms, m(115));
        assert_eq!(s[0].minutes, 25);
    }

    #[test]
    fn skip_and_stop_log_partial_intervals() {
        let (t, s) = start().apply(Action::Skip, m(10), &S);
        assert_eq!(t.phase, Phase::ShortBreak);
        assert_eq!((s[0].minutes, s[0].completed), (10, false));
        assert_eq!(
            t.cycle_done, 0,
            "a skipped work interval doesn't count towards a long break"
        );
        let (t, s) = t.apply(Action::Skip, m(11), &S);
        assert_eq!((t.phase, t.running_since), (Phase::Work, None));
        assert_eq!(s[0].kind, SessionKind::Break);
        let (t, s) = start().apply(Action::Stop, m(7), &S);
        assert_eq!(t, Timer::idle());
        assert_eq!((s[0].minutes, s[0].completed), (7, false));
    }

    #[test]
    fn abandoning_within_a_minute_logs_nothing() {
        let (_, s) = start().apply(Action::Stop, T0 + 30_000, &S);
        assert!(s.is_empty());
        let (t, s) = start().apply(
            Action::Start {
                task_id: Some("other".into()),
            },
            T0 + 20_000,
            &S,
        );
        assert!(s.is_empty());
        assert_eq!(t.task_id.as_deref(), Some("other"));
    }

    #[test]
    fn switching_task_mid_work_logs_the_interrupted_interval() {
        let (t, s) = start().apply(
            Action::Start {
                task_id: Some("other".into()),
            },
            m(12),
            &S,
        );
        assert_eq!(
            (s[0].task_id.as_deref(), s[0].minutes, s[0].completed),
            (Some("task"), 12, false)
        );
        assert_eq!((t.phase, t.remaining(m(12))), (Phase::Work, 25 * MIN));
    }
}
