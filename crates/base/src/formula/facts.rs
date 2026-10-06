//! Facts: a snapshot of the world, the input derivation reads formulas
//! against.

use std::collections::HashSet;

use jiff::{Timestamp, tz::TimeZone};

use crate::ids::{ContextId, PlaceId};

/// The world at one moment: the clock, and what I have declared about where
/// I am and what I am doing. Passed into derivation, never stored in the
/// graph, so `base` never reads a clock itself. Resource balances are
/// graph data and are read from the graph.
#[derive(Clone, Debug)]
pub struct Facts {
  /// The instant a derivation is computed at. One instant for the whole
  /// derivation, so every atom sees the same time.
  pub now: Timestamp,
  /// The zone floating [`Moment`](super::Moment)s are read in.
  pub zone: TimeZone,
  /// The places I am at. Listing the innermost is enough: an `At` atom
  /// also holds for every place these lie within.
  pub places: HashSet<PlaceId>,
  /// The active contexts.
  pub contexts: HashSet<ContextId>,
  /// When my declared free time ends, if I declared it.
  pub free_until: Option<Timestamp>,
}

impl Facts {
  /// The facts at `now` in `zone`, with nothing declared.
  pub fn new(now: Timestamp, zone: TimeZone) -> Self {
    Self {
      now,
      zone,
      places: HashSet::new(),
      contexts: HashSet::new(),
      free_until: None,
    }
  }
}
