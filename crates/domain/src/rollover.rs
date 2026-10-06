//! End-of-day behaviour, driven by task-type configuration (SPEC §6.2c).

/// How a task type behaves when a planned task is not completed by day end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DayEndBehavior {
    /// Rolls over to the next day until completed.
    Carry,
    /// Marked missed; does not carry over.
    Expire,
    /// Carries on within its window (flexible routines, Phase 3).
    Window,
    /// Carries on until its due date, then shows as overdue (Phase 3).
    Deadline,
}

impl DayEndBehavior {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "carry" => Self::Carry,
            "expire" => Self::Expire,
            "window" => Self::Window,
            "deadline" => Self::Deadline,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Carry => "carry",
            Self::Expire => "expire",
            Self::Window => "window",
            Self::Deadline => "deadline",
        }
    }
}

/// What happens to an unfinished planned task when its day has ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Move the day-plan entry to the new day and bump the carry counter.
    CarryTo,
    /// Mark the task missed and leave the entry on its original day.
    Miss,
    /// Leave it alone (e.g. blocked tasks never miss).
    Keep,
}

/// Decide the outcome for an open task whose planned day has passed.
/// Window and deadline types carry over until their window/deadline logic lands
/// (Phase 3); they never miss early.
pub fn day_end_outcome(behavior: DayEndBehavior, blocked: bool) -> Outcome {
    if blocked {
        return Outcome::Keep;
    }
    match behavior {
        DayEndBehavior::Carry | DayEndBehavior::Window | DayEndBehavior::Deadline => {
            Outcome::CarryTo
        }
        DayEndBehavior::Expire => Outcome::Miss,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outcomes() {
        assert_eq!(
            day_end_outcome(DayEndBehavior::Carry, false),
            Outcome::CarryTo
        );
        assert_eq!(
            day_end_outcome(DayEndBehavior::Expire, false),
            Outcome::Miss
        );
        assert_eq!(day_end_outcome(DayEndBehavior::Expire, true), Outcome::Keep);
        assert_eq!(day_end_outcome(DayEndBehavior::Carry, true), Outcome::Keep);
    }

    #[test]
    fn parse_roundtrip() {
        for b in [
            DayEndBehavior::Carry,
            DayEndBehavior::Expire,
            DayEndBehavior::Window,
            DayEndBehavior::Deadline,
        ] {
            assert_eq!(DayEndBehavior::parse(b.as_str()), Some(b));
        }
    }
}
