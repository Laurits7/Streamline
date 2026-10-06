//! Pure domain logic for Streamline: no IO, no database, no HTTP.
//! Everything here is deterministic and unit-tested.

pub mod order;
pub mod rollover;
pub mod time;
