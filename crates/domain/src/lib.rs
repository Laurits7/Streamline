//! Pure domain logic for Streamline: no IO, no database, no HTTP.
//! Everything here is deterministic and unit-tested.

pub mod order;
pub mod planning;
pub mod rollover;
pub mod time;
pub mod tree;
