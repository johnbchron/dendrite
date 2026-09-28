//! Evaluating a schedule's spans at an instant.
//!
//! Spans are floating civil time, so each is laid out as instants in the
//! facts' zone — where DST moves them — and the open windows are merged.
//! Weekly spans repeat forever, so they are laid out over a look-ahead of
//! a little more than a week, which always contains the next window.

use jiff::{Timestamp, civil::Date, tz::TimeZone};

use super::literal::instant;
use crate::referent::Span;

/// The days of weekly windows laid out, starting the day before today: a
/// window that opened yesterday may still be open, and seven more days
/// always reach the next opening.
const LOOK_AHEAD_DAYS: usize = 10;

/// Whether `now` is inside any of `spans`, and the next instant that could
/// change that (see [`Truth::until`](super::Truth::until)).
pub(super) fn truth(
  spans: &[Span],
  now: Timestamp,
  zone: &TimeZone,
) -> (bool, Option<Timestamp>) {
  let first = zone.to_datetime(now).date();
  let first = first.yesterday().unwrap_or(first);
  let dates: Vec<Date> =
    core::iter::successors(Some(first), |d| d.tomorrow().ok())
      .take(LOOK_AHEAD_DAYS)
      .collect();

  let mut windows = Vec::new();
  let mut repeats = false;
  for span in spans {
    match span {
      Span::Once { start, end } => {
        windows.push((start.instant(zone), end.instant(zone)));
      }
      Span::Weekly { days, start, end } => {
        repeats |= !days.is_empty();
        for date in dates.iter().filter(|d| days.contains(d.weekday())) {
          let closes_on = if end <= start {
            date.tomorrow().unwrap_or(*date)
          } else {
            *date
          };
          windows.push((
            instant(date.to_datetime(start.civil()), zone),
            instant(closes_on.to_datetime(end.civil()), zone),
          ));
        }
      }
    }
  }

  // Past the look-ahead, weekly windows were not laid out, so nothing is
  // known there: report its end as the latest the truth is known until.
  let limit = repeats.then(|| {
    let last = *dates.last().unwrap();
    instant(
      last
        .tomorrow()
        .unwrap_or(last)
        .to_datetime(Default::default()),
      zone,
    )
  });

  windows.retain(|(start, end)| start < end);
  windows.sort_unstable();
  let mut merged: Vec<(Timestamp, Timestamp)> = Vec::new();
  for (start, end) in windows {
    match merged.last_mut() {
      Some(last) if start <= last.1 => last.1 = last.1.max(end),
      _ => merged.push((start, end)),
    }
  }

  let open = merged
    .iter()
    .find(|(start, end)| *start <= now && now < *end);
  let (holds, next) = match open {
    Some((_, end)) => (true, Some(*end)),
    None => (false, merged.iter().map(|w| w.0).find(|start| *start > now)),
  };
  let until = match (next, limit) {
    (Some(next), Some(limit)) => Some(next.min(limit)),
    (next, limit) => next.or(limit),
  };
  (holds, until)
}
