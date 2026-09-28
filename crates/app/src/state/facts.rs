//! Facts: what the app tells derivation about the world — the clock, and
//! what I have declared about where I am, what I am doing and how long I
//! have (plans/formula-conditions.md, "Facts and derivation").
//!
//! Declared facts are observations of this device at this moment, so they
//! live in preferences, not the event log: they survive a restart but never
//! reach undo or sync. Changing one, or the clock passing the horizon, bumps
//! [`AppState::facts_revision`], which is what derived state is cached on
//! beside the graph revision. Layout never depends on facts.

use std::{collections::BTreeSet, time::Duration};

use base::{ContextId, Facts, Moment, PlaceId};
use jiff::{SignedDuration, Timestamp, civil::Date};
use session::Session;

use super::AppState;
use crate::{
  focus::FieldKey,
  formula::{describe, phrase},
};

/// The longest the app goes without looking at the clock, to catch what a
/// timer set for the horizon cannot: sleep, a changed clock, a new zone.
pub const SAFETY_TICK: Duration = Duration::from_secs(60);

/// Preference keys the declared facts are kept under.
const PLACE: &str = "facts.place";
const CONTEXTS: &str = "facts.contexts";
const FREE_UNTIL: &str = "facts.free_until";

/// What I have declared: where I am, the active contexts, and when my free
/// time ends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct Declared {
  place:      Option<PlaceId>,
  contexts:   BTreeSet<ContextId>,
  free_until: Option<Timestamp>,
}

impl Declared {
  /// The facts `store`'s preferences record; anything unreadable is taken
  /// as undeclared.
  pub(super) fn load(store: &Session) -> Self {
    let get = |key| store.setting(key).ok().flatten().unwrap_or_default();
    Self {
      place:      get(PLACE).parse().ok(),
      contexts:   get(CONTEXTS)
        .split(',')
        .filter_map(|c| c.parse().ok())
        .collect(),
      free_until: get(FREE_UNTIL).parse().ok(),
    }
  }

  /// Write every declared fact to `store`'s preferences.
  fn save(&self, store: &Session) {
    let contexts: Vec<String> =
      self.contexts.iter().map(ToString::to_string).collect();
    let writes = [
      (PLACE, self.place.map(|p| p.to_string()).unwrap_or_default()),
      (CONTEXTS, contexts.join(",")),
      (
        FREE_UNTIL,
        self.free_until.map(|t| t.to_string()).unwrap_or_default(),
      ),
    ];
    for (key, value) in writes {
      if let Err(e) = store.set_setting(key, &value) {
        eprintln!("saving {key} failed: {e}");
      }
    }
  }
}

impl AppState {
  /// The facts at this moment.
  pub(super) fn facts(&self) -> Facts {
    let mut facts = Facts::new(self.clock.now(), self.clock.zone());
    facts.places.extend(self.declared.place);
    facts
      .contexts
      .extend(self.declared.contexts.iter().copied());
    facts.free_until = self.declared.free_until;
    facts
  }

  /// Today's date where I am.
  pub(super) fn today(&self) -> Date {
    self.clock.zone().to_datetime(self.clock.now()).date()
  }

  /// A counter that changes whenever the facts derived state was computed
  /// from may have: a declared fact changed, or time moved past the
  /// horizon.
  pub fn facts_revision(&self) -> u64 { self.facts_revision }

  /// Where I declared I am.
  pub fn place(&self) -> Option<PlaceId> { self.declared.place }

  /// The contexts I declared active.
  pub fn active_contexts(&self) -> &BTreeSet<ContextId> {
    &self.declared.contexts
  }

  /// When I declared my free time ends.
  pub fn free_until(&self) -> Option<Timestamp> { self.declared.free_until }

  /// Declare where I am (`None`: nowhere I have named).
  pub fn set_place(&mut self, place: Option<PlaceId>) {
    self.declare(|d| d.place = place);
  }

  /// Turn a context on or off.
  pub fn toggle_context(&mut self, context: ContextId) {
    self.declare(|d| {
      if !d.contexts.remove(&context) {
        d.contexts.insert(context);
      }
    });
  }

  /// Declare when my free time ends (`None`: unknown).
  pub fn set_free_until(&mut self, until: Option<Timestamp>) {
    self.declare(|d| d.free_until = until);
  }

  /// Declare that I am free for `minutes` from now.
  pub fn set_free_for(&mut self, minutes: u32) {
    let until = self
      .clock
      .now()
      .checked_add(SignedDuration::from_mins(minutes.into()))
      .ok();
    self.set_free_until(until);
  }

  /// Declared free time in words: "Until 15:30 · 1 h 20 min left", or
  /// "Ended at 15:30"; `None` when none is declared.
  pub fn free_summary(&self) -> Option<String> {
    let until = self.declared.free_until?;
    let (now, zone) = (self.clock.now(), self.clock.zone());
    let at =
      describe::moment(Moment::new(zone.to_datetime(until)), self.today());
    // Today's date goes without saying.
    let at = at
      .strip_prefix(&describe::moment(Moment::on(self.today()), self.today()))
      .map(str::trim)
      .filter(|time| !time.is_empty())
      .map_or(at.clone(), str::to_string);
    let left = until.duration_since(now).as_mins();
    Some(if left > 0 {
      format!(
        "Until {at} \u{b7} {} left",
        describe::minutes(left.min(u32::MAX.into()) as u32)
      )
    } else {
      format!("Ended at {at}")
    })
  }

  /// The free time field changed.
  pub fn set_free_text(&mut self, text: String) { self.free_draft = text; }

  /// Enter in the free time field: "15:30" or "3pm" is free until then,
  /// "1h" is free for that long. The field clears once it is understood.
  pub fn apply_free_text(&mut self) {
    let (now, zone) = (self.clock.now(), self.clock.zone());
    if let Some(until) = phrase::until(&self.free_draft, now, &zone) {
      self.set_free_until(Some(until));
      self.free_draft.clear();
    }
  }

  /// Open the Now tray at its free time field.
  pub fn edit_free_time(&mut self) {
    self.now_open = true;
    self.focus_requests.request(FieldKey::FreeUntil);
  }

  /// Every place, by name, for the place picker.
  pub fn place_choices(&self) -> Vec<(PlaceId, String)> {
    let store = self.lock();
    let mut places: Vec<(PlaceId, String)> = store
      .graph()
      .places()
      .map(|p| (p.id, p.name.clone()))
      .collect();
    places.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    places
  }

  /// The name of where I declared I am.
  pub fn place_name(&self) -> Option<String> {
    let place = self.declared.place?;
    self.lock().graph().place(place).map(|p| p.name.clone())
  }

  /// Every context, by name, and whether it is on, for the context bar.
  pub fn context_chips(&self) -> Vec<(ContextId, String, bool)> {
    let store = self.lock();
    let mut chips: Vec<(ContextId, String, bool)> = store
      .graph()
      .contexts()
      .map(|c| (c.id, c.name.clone(), self.declared.contexts.contains(&c.id)))
      .collect();
    chips.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    chips
  }

  /// Whether the Now tray's place picker is showing.
  pub fn place_picker_open(&self) -> bool { self.place_picker }

  /// Show or hide the place picker, opening the tray to show it (C).
  pub fn toggle_place_picker(&mut self) {
    self.place_picker = !self.place_picker;
    if self.place_picker {
      self.now_open = true;
    }
  }

  /// Choose where I am from the picker, and close it.
  pub fn pick_place(&mut self, place: Option<PlaceId>) {
    self.set_place(place);
    self.place_picker = false;
  }

  /// Change the declared facts, save them, and invalidate what was derived
  /// from the old ones.
  fn declare(&mut self, change: impl FnOnce(&mut Declared)) {
    let before = self.declared.clone();
    change(&mut self.declared);
    if self.declared != before {
      self.declared.save(&self.lock());
      self.facts_revision += 1;
    }
  }

  /// Look at the clock: when time has passed the horizon, or the clock or
  /// zone moved in a way a timer would not notice, derived state is stale.
  /// The UI calls this when the timer from [`AppState::wake`] fires.
  pub fn tick(&mut self) {
    let (now, zone) = (self.clock.now(), self.clock.zone());
    let stale = {
      let store = self.lock();
      let d = self.derivations(&store);
      d.derived.horizon().is_some_and(|h| now >= h)
        || now < d.facts.now
        || zone != d.facts.zone
    };
    if stale {
      self.facts_revision += 1;
    }
    self.checked_at = now;
  }

  /// When the UI should next call [`AppState::tick`]: the horizon, or a
  /// minute after the last look at the clock if that is sooner. The key
  /// names the moment, so a timer re-arms only when the moment moves.
  pub fn wake(&self) -> (u64, Duration) {
    let fallback = self
      .checked_at
      .checked_add(SignedDuration::try_from(SAFETY_TICK).unwrap_or_default())
      .unwrap_or(Timestamp::MAX);
    let horizon = {
      let store = self.lock();
      self.derivations(&store).derived.horizon()
    };
    let at = horizon.map_or(fallback, |h| h.min(fallback));
    (
      at.as_nanosecond() as u64,
      describe::until(self.clock.now(), at),
    )
  }
}
