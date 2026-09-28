//! Formula conditions in the app (plans/formula-conditions.md, step F3):
//! the clock, declared facts, the horizon, and what the tray and the
//! inspector say.

use std::{
  sync::{Arc, Mutex},
  time::Duration,
};

use base::{Atom, ContextId, Moment, PlaceId};
use jiff::{Timestamp, tz::TimeZone};

use super::*;
use crate::formula::Clock;

/// A clock that reads whatever a test sets it to, in UTC.
#[derive(Clone)]
struct ManualClock(Arc<Mutex<Timestamp>>);

impl ManualClock {
  fn at(s: &str) -> Self { Self(Arc::new(Mutex::new(s.parse().unwrap()))) }

  fn set(&self, s: &str) { *self.0.lock().unwrap() = s.parse().unwrap(); }
}

impl Clock for ManualClock {
  fn now(&self) -> Timestamp { *self.0.lock().unwrap() }

  fn zone(&self) -> TimeZone { TimeZone::UTC }
}

fn opens() -> Atom {
  Atom::After {
    at: "2026-10-01T09:00".parse::<Moment>().unwrap(),
  }
}

/// The demo graph plus "Call the bank", which needs it to be after 9am on
/// Thursday 1 October, seen on the Monday before.
fn bank_state() -> (AppState, ManualClock, NodeId) {
  let mut store = demo_store();
  let bank = NodeId::new();
  let mut events = vec![Event::NodeAdded {
    node:       bank,
    kind:       NodeKind::task(),
    name:       "Call the bank".into(),
    order_hint: 0.0,
  }];
  events.extend(opens().require(store.graph(), bank, EdgeId::new()));
  store.commit(events).unwrap();
  let clock = ManualClock::at("2026-09-28T12:00Z");
  (AppState::with_clock(store, clock.clone()), clock, bank)
}

/// Every node the tray lists as ready.
fn ready(state: &AppState) -> Vec<String> {
  let (groups, _) = state.now();
  groups
    .into_iter()
    .flat_map(|g| g.items.into_iter().map(|(_, name)| name))
    .collect()
}

fn state_of(state: &AppState, node: NodeId) -> Option<NodeState> {
  let scene = state.scene();
  scene.nodes.iter().find(|n| n.node == node).map(|n| n.state)
}

#[test]
fn passing_the_horizon_changes_the_now_tray_and_nothing_else_recomputes() {
  let (mut state, clock, bank) = bank_state();
  let derivations = |s: &AppState| s.derivations(&s.lock());

  assert!(!ready(&state).contains(&"Call the bank".to_string()));
  let soon = state.soon();
  assert_eq!(soon.len(), 1);
  assert_eq!((soon[0].node, soon[0].when.as_str()), (bank, "in 2 days"));
  // Days away: the next look at the clock is the minute's safety tick.
  assert_eq!(state.wake().1, SAFETY_TICK);

  let (d0, s0) = (derivations(&state), state.scene());
  let revision = state.facts_revision();

  // Just short of it, nothing changes, and the timer aims at the horizon.
  clock.set("2026-10-01T08:59:59Z");
  state.tick();
  assert_eq!(state.facts_revision(), revision);
  assert!(Arc::ptr_eq(&d0, &derivations(&state)));
  assert!(Arc::ptr_eq(&s0, &state.scene()));
  assert_eq!(state.wake().1, Duration::from_secs(1));

  // At the horizon, the tray and the node's state change; layout does not.
  clock.set("2026-10-01T09:00:00Z");
  state.tick();
  assert_ne!(state.facts_revision(), revision);
  let d1 = derivations(&state);
  assert!(!Arc::ptr_eq(&d0, &d1));
  assert!(Arc::ptr_eq(&d0.layout, &d1.layout), "layout was recomputed");
  assert!(ready(&state).contains(&"Call the bank".to_string()));
  assert!(state.soon().is_empty());
  assert!(!Arc::ptr_eq(&s0, &state.scene()));
  assert_eq!(state_of(&state, bank), Some(NodeState::Ready));
  assert_eq!(d1.derived.horizon(), None);
  assert_eq!(state.wake().1, SAFETY_TICK);
}

#[test]
fn a_clock_turned_back_is_noticed() {
  let (mut state, clock, bank) = bank_state();
  clock.set("2026-10-02T12:00Z");
  state.tick();
  assert_eq!(state_of(&state, bank), Some(NodeState::Ready));

  // No horizon to pass, but the clock went backwards past it.
  clock.set("2026-09-30T12:00Z");
  state.tick();
  assert_eq!(state_of(&state, bank), Some(NodeState::Blocked));
}

#[test]
fn the_inspector_reads_formulas_through_derived_state() {
  let (mut state, _, bank) = bank_state();
  let condition = opens().node_id();

  state.select(Some(bank));
  let info = state.selected_info().unwrap();
  assert_eq!(info.state, NodeState::Blocked);
  assert_eq!(
    info.reason,
    Reason::WaitingOn(vec![(condition, "After Thu 1 Oct 09:00".into())])
  );
  assert_eq!(info.requirements[0].name, "After Thu 1 Oct 09:00");

  state.select(Some(condition));
  let info = state.selected_info().unwrap();
  assert_eq!(info.reason.sentence(), "Opens Thu 1 Oct 09:00, in 2 days.");
  assert_eq!(info.primary, Primary::Automatic);
  assert!(!info.primary.enabled());

  // Nothing to press: the toggle commits nothing.
  let before = state.lock().event_count().unwrap();
  state.toggle_selected();
  assert_eq!(state.lock().event_count().unwrap(), before);
}

#[test]
fn declared_facts_gate_readiness_and_are_kept_in_preferences() {
  let mut store = demo_store();
  let (home, online) = (PlaceId::new(), ContextId::new());
  let (nails, email) = (NodeId::new(), NodeId::new());
  let task = |node, name: &str| Event::NodeAdded {
    node,
    kind: NodeKind::task(),
    name: name.into(),
    order_hint: 0.0,
  };
  let mut events = vec![
    Event::PlaceDefined {
      place:  home,
      name:   "Home".into(),
      within: None,
    },
    Event::ContextDefined {
      context: online,
      name:    "Online".into(),
    },
    task(nails, "Hang the shelf"),
    task(email, "Send the email"),
  ];
  let g = store.graph().clone();
  events.extend(Atom::At { place: home }.require(&g, nails, EdgeId::new()));
  events.extend(Atom::In { context: online }.require(&g, email, EdgeId::new()));
  events.extend(Atom::Free { at_least: 60 }.require(&g, email, EdgeId::new()));
  store.commit(events).unwrap();
  let clock = ManualClock::at("2026-09-28T12:00Z");
  let mut state = AppState::with_clock(store, clock);
  let is_ready = |s: &AppState, n| s.derivations(&s.lock()).derived.is_ready(n);
  assert!(!is_ready(&state, nails));

  let revision = state.facts_revision();
  state.set_place(Some(home));
  assert_ne!(state.facts_revision(), revision);
  assert!(is_ready(&state, nails));
  assert_eq!(
    state.lock().setting("facts.place").unwrap(),
    Some(home.to_string())
  );

  // Setting what is already set changes nothing.
  let revision = state.facts_revision();
  state.set_place(Some(home));
  assert_eq!(state.facts_revision(), revision);

  // Both of the email's requirements, and free time runs out on its own.
  state.toggle_context(online);
  assert!(!is_ready(&state, email));
  state.set_free_for(90);
  assert!(is_ready(&state, email));
  let horizon = state.derivations(&state.lock()).derived.horizon();
  let expect: Timestamp = "2026-09-28T12:30Z".parse().unwrap();
  assert_eq!(horizon, Some(expect));

  state.toggle_context(online);
  assert!(state.active_contexts().is_empty());
  assert!(!is_ready(&state, email));
}

#[test]
fn declared_facts_are_restored_on_opening() {
  let store = demo_store();
  let home = PlaceId::new();
  let until: Timestamp = "2026-09-28T15:30Z".parse().unwrap();
  store.set_setting("facts.place", &home.to_string()).unwrap();
  store
    .set_setting("facts.free_until", &until.to_string())
    .unwrap();
  store.set_setting("facts.contexts", "not an id").unwrap();

  let state = AppState::new(store);
  assert_eq!(state.place(), Some(home));
  assert_eq!(state.free_until(), Some(until));
  assert!(state.active_contexts().is_empty(), "garbage reads as unset");
}

#[test]
fn a_typed_phrase_requires_one_shared_node() {
  let (mut state, ..) = bank_state();
  let backend = node_named(&state, "Build backend");
  let frontend = node_named(&state, "Build frontend");

  state.select(Some(backend));
  state.begin_link();
  state.link_filter = "at hardware store".into();
  let offers = state.atom_offers();
  let new = offers
    .iter()
    .find(|o| o.offer.label == "At Hardware store (new place)")
    .unwrap_or_else(|| panic!("{offers:?}"));
  assert_eq!(new.used_by, 0);
  state.require_offer(new.offer.clone());
  assert_eq!(state.lock().undo_label(), Some("add condition"));

  // The second time, the place and the node exist: offered, and linked.
  state.select(Some(frontend));
  state.begin_link();
  state.link_filter = "at hardware".into();
  let offers = state.atom_offers();
  assert_eq!(offers[0].offer.label, "At Hardware store");
  assert_eq!(offers[0].used_by, 1);
  assert!(offers[0].offer.define.is_empty());
  state.require_offer(offers[0].offer.clone());
  assert_eq!(state.lock().undo_label(), Some("add requirement"));

  let store = state.lock();
  let at: Vec<_> = store
    .graph()
    .nodes()
    .filter(|n| matches!(n.kind.atom(), Some(Atom::At { .. })))
    .collect();
  assert_eq!(at.len(), 1);
  assert_eq!(store.graph().dependents_of(at[0].id).count(), 2);
  assert_eq!(store.graph().places().count(), 1);

  // A formula the selection already requires is not offered again.
  drop(store);
  assert!(
    state
      .atom_offers()
      .iter()
      .all(|o| o.offer.label != "At Hardware store")
  );
}
