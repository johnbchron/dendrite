//! Formula conditions on the app side: the clock facts are read against,
//! atoms in words, and reading typed phrases as atoms
//! (plans/formula-conditions.md, step F3).
//!
//! `base` evaluates atoms against facts it is handed and never reads a
//! clock; the [`Clock`] here is where the app gets the time, so a test can
//! hand it a time of its choosing instead.

pub mod describe;
pub mod phrase;

use jiff::{Timestamp, tz::TimeZone};

/// Where the app reads the time and the zone floating moments are read in.
pub trait Clock: Send + Sync {
  /// The current instant.
  fn now(&self) -> Timestamp;

  /// The zone the user is in.
  fn zone(&self) -> TimeZone;
}

/// The system clock and time zone.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
  fn now(&self) -> Timestamp { Timestamp::now() }

  fn zone(&self) -> TimeZone { TimeZone::system() }
}
