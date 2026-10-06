//! Referents: the named, editable things formula atoms point at
//! (plans/formula-conditions.md, "Referents").
//!
//! An atom names a referent by id rather than embedding its definition, so
//! editing the definition — moving house, renaming a budget — changes no
//! atom's identity. Referents are graph data, carried by the event log like
//! nodes and quests.

use serde::{Deserialize, Serialize};

use crate::{
  formula::{Amount, Moment, TimeOfDay, WeekdaySet},
  ids::{ContextId, PlaceId, ResourceId, ScheduleId},
};

/// Somewhere I can be: Home, the hardware store, Chicago.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Place {
  /// Stable identity.
  pub id: PlaceId,
  /// Display name; `At` atoms render with it.
  pub name: String,
  /// The place this one lies in. Being here means being there too, which
  /// is how a group ("Errands") holds at any of its members.
  pub within: Option<PlaceId>,
}

/// Something countable I have some of: money, contractor hours, batteries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Resource {
  /// Stable identity.
  pub id: ResourceId,
  /// Display name.
  pub name: String,
  /// What an [`Amount`] of this counts, for display and parsing.
  pub unit: Unit,
  /// How much I declare I have, in the unit's smallest step.
  pub balance: Amount,
}

/// What a [`Resource`]'s amounts count.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "unit", rename_all = "snake_case")]
pub enum Unit {
  /// Money, counted in minor units (cents for two `minor_digits`).
  Money {
    /// ISO 4217 code, such as `USD`.
    currency: String,
    /// Digits after the decimal point.
    minor_digits: u8,
  },
  /// Time, counted in minutes.
  Minutes,
  /// Whole items.
  Count {
    /// What one item is called: "battery".
    noun: String,
  },
}

/// A named set of windows in time: "Business hours", "Conference week".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Schedule {
  /// Stable identity.
  pub id: ScheduleId,
  /// Display name.
  pub name: String,
  /// The windows; the schedule is open during any of them.
  pub spans: Vec<Span>,
}

/// One window, or a weekly run of them, in floating civil time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "span", rename_all = "snake_case")]
pub enum Span {
  /// A one-off window, `[start, end)`.
  Once {
    /// When it opens.
    start: Moment,
    /// When it closes.
    end: Moment,
  },
  /// On each of `days`, from `start` to `end`. An `end` at or before
  /// `start` wraps past midnight into the next day ("Fri 22:00 – 02:00"),
  /// so `00:00 – 00:00` is the whole day.
  Weekly {
    /// The days a window opens on.
    days: WeekdaySet,
    /// When each window opens.
    start: TimeOfDay,
    /// When each window closes.
    end: TimeOfDay,
  },
}

/// A circumstance I toggle by hand: "@computer", "online", "with Sam".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Context {
  /// Stable identity.
  pub id: ContextId,
  /// Display name.
  pub name: String,
}
