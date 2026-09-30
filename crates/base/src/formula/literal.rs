//! Canonical literals: the values an [`Atom`](super::Atom) is built from.
//!
//! Every literal has exactly one representation, so atoms that mean the
//! same thing are equal values and hash to the same node id. No floats (no
//! `0.1 + 0.2` duplicates, no `NaN`), no sub-minute times, no zones.

use core::{fmt, str::FromStr};

use jiff::{
  Timestamp,
  civil::{Date, DateTime, Time, Weekday},
  tz::TimeZone,
};
use serde::{Deserialize, Serialize};
use serde_with::{DeserializeFromStr, SerializeDisplay};

/// A span of time in whole minutes. Free time is never meaningful below a
/// minute.
pub type Minutes = u32;

/// A quantity in a resource's smallest unit: cents, minutes, items.
pub type Amount = i64;

/// A civil date-time at minute precision with no zone: `2026-10-01T09:00`.
///
/// Moments float: they are read in whatever zone the facts carry, so "9am"
/// means 9am wherever I am.
#[derive(
  Clone,
  Copy,
  PartialEq,
  Eq,
  PartialOrd,
  Ord,
  Hash,
  SerializeDisplay,
  DeserializeFromStr,
)]
pub struct Moment(DateTime);

impl Moment {
  /// The moment at `dt`, with any seconds dropped.
  pub fn new(dt: DateTime) -> Self {
    Self(dt.date().to_datetime(minute_time(dt.time())))
  }

  /// Midnight at the start of `date`.
  pub fn on(date: Date) -> Self { Self(date.to_datetime(Time::midnight())) }

  /// The civil date-time.
  pub fn civil(self) -> DateTime { self.0 }

  /// The instant this moment names in `zone`. A civil time skipped by a
  /// DST gap reads as the instant just after the gap; one repeated by a
  /// fold reads as its first occurrence.
  pub fn instant(self, zone: &TimeZone) -> Timestamp { instant(self.0, zone) }

  /// Fixed-width encoding for [`Atom::canonical_bytes`](super::Atom).
  pub(crate) fn canonical_bytes(self) -> [u8; 6] {
    let [y0, y1] = self.0.year().to_be_bytes();
    [
      y0,
      y1,
      self.0.month() as u8,
      self.0.day() as u8,
      self.0.hour() as u8,
      self.0.minute() as u8,
    ]
  }
}

/// The instant `dt` names in `zone`, pinned to the ends of time when it
/// falls outside the range a timestamp can hold.
pub(crate) fn instant(dt: DateTime, zone: &TimeZone) -> Timestamp {
  zone.to_timestamp(dt).unwrap_or(if dt.year() < 0 {
    Timestamp::MIN
  } else {
    Timestamp::MAX
  })
}

impl fmt::Display for Moment {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}T{}", self.0.date(), TimeOfDay(self.0.time()))
  }
}

impl fmt::Debug for Moment {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "Moment({self})")
  }
}

impl FromStr for Moment {
  type Err = LiteralError;

  /// Parses `2026-10-01T09:00`, or a bare date as its midnight. Seconds
  /// are refused rather than dropped, so a value that parses round-trips.
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let dt: DateTime = s.parse().map_err(|_| LiteralError::Moment)?;
    minute_exact(dt.time()).ok_or(LiteralError::Moment)?;
    Ok(Self(dt))
  }
}

/// A time of day at minute precision: `09:00`.
#[derive(
  Clone,
  Copy,
  PartialEq,
  Eq,
  PartialOrd,
  Ord,
  Hash,
  SerializeDisplay,
  DeserializeFromStr,
)]
pub struct TimeOfDay(Time);

impl TimeOfDay {
  /// Midnight, `00:00`.
  pub const MIDNIGHT: Self = Self(Time::midnight());

  /// `hour:minute`, or `None` if either is out of range.
  pub fn new(hour: i8, minute: i8) -> Option<Self> {
    Time::new(hour, minute, 0, 0).ok().map(Self)
  }

  /// The civil time.
  pub fn civil(self) -> Time { self.0 }
}

impl fmt::Display for TimeOfDay {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{:02}:{:02}", self.0.hour(), self.0.minute())
  }
}

impl fmt::Debug for TimeOfDay {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "TimeOfDay({self})")
  }
}

impl FromStr for TimeOfDay {
  type Err = LiteralError;

  /// Parses `09:00`; seconds are refused, as for [`Moment`].
  fn from_str(s: &str) -> Result<Self, Self::Err> {
    let time: Time = s.parse().map_err(|_| LiteralError::TimeOfDay)?;
    minute_exact(time).map(Self).ok_or(LiteralError::TimeOfDay)
  }
}

/// `time` with its seconds dropped.
fn minute_time(time: Time) -> Time {
  Time::constant(time.hour(), time.minute(), 0, 0)
}

/// `time`, if it has no seconds to drop.
fn minute_exact(time: Time) -> Option<Time> {
  (time == minute_time(time)).then_some(time)
}

/// A set of weekdays, one bit each from Monday (bit 0) to Sunday (bit 6).
#[derive(Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct WeekdaySet(u8);

impl WeekdaySet {
  /// Monday to Sunday.
  pub const EVERY_DAY: Self = Self(0b111_1111);
  /// Saturday and Sunday.
  pub const WEEKEND: Self = Self(0b110_0000);
  /// Monday to Friday.
  pub const WORKDAYS: Self = Self(0b001_1111);

  /// Whether `day` is in the set.
  pub fn contains(self, day: Weekday) -> bool { self.0 & bit(day) != 0 }

  /// Whether the set has no days.
  pub fn is_empty(self) -> bool { self.0 & Self::EVERY_DAY.0 == 0 }
}

impl FromIterator<Weekday> for WeekdaySet {
  fn from_iter<I: IntoIterator<Item = Weekday>>(days: I) -> Self {
    Self(days.into_iter().fold(0, |set, day| set | bit(day)))
  }
}

impl fmt::Debug for WeekdaySet {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    let days = (0..7)
      .filter_map(|i| Weekday::from_monday_zero_offset(i).ok())
      .filter(|d| self.contains(*d));
    f.debug_set().entries(days).finish()
  }
}

fn bit(day: Weekday) -> u8 { 1 << day.to_monday_zero_offset() }

/// A literal that is not in its canonical form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum LiteralError {
  /// Not `YYYY-MM-DDTHH:MM` or a bare date.
  #[error("expected a moment like 2026-10-01T09:00")]
  Moment,
  /// Not `HH:MM`.
  #[error("expected a time of day like 09:00")]
  TimeOfDay,
}
