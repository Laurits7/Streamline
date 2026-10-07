//! Pure domain logic for Streamline: no IO, no database, no HTTP.
//! Everything here is deterministic and unit-tested.

pub mod calendar;
pub mod checklist;
pub mod conflicts;
pub mod deps;
pub mod focus;
pub mod goals;
pub mod marks;
pub mod occasions;
pub mod order;
pub mod planner;
pub mod planning;
pub mod recurrence;
pub mod rollover;
pub mod time;
pub mod tracking;
pub mod tree;
pub mod workflow;
