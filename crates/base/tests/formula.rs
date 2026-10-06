//! Formula conditions (plans/formula-conditions.md, step F1): canonical
//! literals and content-addressed ids, deduplication through the reducer,
//! atom evaluation against facts, and derivation with a horizon.

use std::collections::HashSet;

use base::{
  Atom, ContextId, Derived, EdgeId, Event, Explanation, Facts, Graph, Moment,
  NodeId, NodeKind, NodeState, PlaceId, ResourceId, ScheduleId, Span,
  TimeOfDay, Unit, WeekdaySet, apply_batch, cyclic_nodes,
};
use jiff::{
  SignedDuration, Timestamp,
  civil::{Weekday, datetime},
  tz::TimeZone,
};

// --- helpers ------------------------------------------------------------

fn nid(n: u128) -> NodeId {
  NodeId::from_u128(n)
}

fn eid(n: u128) -> EdgeId {
  EdgeId::from_u128(n)
}

fn moment(s: &str) -> Moment {
  s.parse().unwrap()
}

fn tod(s: &str) -> TimeOfDay {
  s.parse().unwrap()
}

fn chicago() -> TimeZone {
  TimeZone::get("America/Chicago").unwrap()
}

/// The instant a civil time names in `zone`.
fn at(zone: &TimeZone, s: &str) -> Timestamp {
  moment(s).instant(zone)
}

fn facts(now: Timestamp, zone: &TimeZone) -> Facts {
  Facts::new(now, zone.clone())
}

fn apply(graph: &mut Graph, events: &[Event]) -> Vec<Event> {
  apply_batch(graph, events)
}

fn add_task(graph: &mut Graph, n: u128) {
  apply(
    graph,
    &[Event::NodeAdded {
      node: nid(n),
      kind: NodeKind::task(),
      name: format!("t{n}"),
      order_hint: 0.0,
    }],
  );
}

fn after(s: &str) -> Atom {
  Atom::After { at: moment(s) }
}

/// A graph holding one weekly schedule, and the atom that points at it.
fn weekly(days: WeekdaySet, start: &str, end: &str) -> (Graph, Atom) {
  let schedule = ScheduleId::from_u128(1);
  let mut g = Graph::new();
  apply(
    &mut g,
    &[Event::ScheduleDefined {
      schedule,
      name: "window".into(),
      spans: vec![Span::Weekly {
        days,
        start: tod(start),
        end: tod(end),
      }],
    }],
  );
  (g, Atom::Within { schedule })
}

// --- literals and identity ----------------------------------------------

#[test]
fn literals_have_one_spelling() {
  assert_eq!(moment("2026-10-01T09:00").to_string(), "2026-10-01T09:00");
  assert_eq!(
    moment("2026-10-01T09:00:00").to_string(),
    "2026-10-01T09:00"
  );
  assert_eq!(moment("2026-10-01"), moment("2026-10-01T00:00"));
  assert!("2026-10-01T09:00:30".parse::<Moment>().is_err());
  assert!("2026-10-01T09:00:00.5".parse::<Moment>().is_err());

  assert_eq!(tod("22:00").to_string(), "22:00");
  assert!("22:00:01".parse::<TimeOfDay>().is_err());

  // Constructing from a civil time drops the seconds rather than keeping a
  // second spelling of the same minute.
  let dt = datetime(2026, 10, 1, 9, 0, 59, 999);
  assert_eq!(Moment::new(dt), moment("2026-10-01T09:00"));
}

#[test]
fn formula_nodes_serialize_as_self_describing_json() {
  let e = after("2026-10-01T09:00").node_added(0.0);
  let json = serde_json::to_string(&e).unwrap();
  assert!(
    json.contains(
      r#""source":"formula","atom":{"atom":"after","at":"2026-10-01T09:00"}"#
    ),
    "{json}"
  );
  let back: Event = serde_json::from_str(&json).unwrap();
  assert_eq!(e, back);

  let span = Span::Weekly {
    days: WeekdaySet::WEEKEND,
    start: tod("22:00"),
    end: tod("02:00"),
  };
  let json = serde_json::to_string(&span).unwrap();
  assert_eq!(
    json,
    r#"{"span":"weekly","days":96,"start":"22:00","end":"02:00"}"#
  );
}

/// Formula ids are frozen: these must never change, or every formula node
/// already in a log silently forks from the atoms that name it. The first
/// was checked by hand: BLAKE3 of `01 01 07ea 0a 01 00 00`, low 80 bits.
#[test]
fn node_ids_are_pinned() {
  let pinned = [
    (after("2026-10-01T00:00"), "0000000000SE2HQC2NFK7Q3ZB6"),
    (
      Atom::Before {
        at: moment("2026-10-15T17:30"),
      },
      "0000000000XMHA37C5F9AMCV4C",
    ),
    (
      Atom::Within {
        schedule: ScheduleId::from_u128(7),
      },
      "0000000000GF4GGT3K2H8FYC2Y",
    ),
    (Atom::Free { at_least: 60 }, "0000000000HHCWMTMGZJX5X8WV"),
    (
      Atom::At {
        place: PlaceId::from_u128(1),
      },
      "00000000000336Y8P2MP51DNB3",
    ),
    (
      Atom::Has {
        resource: ResourceId::from_u128(2),
        at_least: 5000,
      },
      "00000000008RAFM2S46P44ZHT1",
    ),
    (
      Atom::In {
        context: ContextId::from_u128(3),
      },
      "0000000000RCEMMP7MJA3RDYVV",
    ),
  ];
  for (atom, id) in pinned {
    assert_eq!(atom.node_id().to_string(), id, "{atom:?}");
  }
}

#[test]
fn formula_ids_are_value_addressed() {
  assert!(after("2026-10-01").node_id().is_value_addressed());
  assert!(!NodeId::new().is_value_addressed());
}

// --- deduplication through the reducer ----------------------------------

#[test]
fn requiring_an_existing_atom_links_to_it() {
  let home = PlaceId::from_u128(1);
  let at_home = Atom::At { place: home };
  let mut g = Graph::new();
  add_task(&mut g, 1);
  add_task(&mut g, 2);

  let first = at_home.require(&g, nid(1), eid(10));
  assert!(matches!(
    first[..],
    [Event::NodeAdded { .. }, Event::EdgeAdded { .. }]
  ));
  apply(&mut g, &first);

  let second = at_home.require(&g, nid(2), eid(11));
  assert!(matches!(second[..], [Event::EdgeAdded { .. }]));
  let before_second = g.clone();
  let undo_second = apply(&mut g, &second);

  let formula: Vec<_> = g.nodes().filter(|n| n.kind.atom().is_some()).collect();
  assert_eq!(formula.len(), 1);
  assert_eq!(formula[0].id, at_home.node_id());
  assert_eq!(g.dependents_of(at_home.node_id()).count(), 2);

  // Undoing the second requirement removes only its edge.
  apply(&mut g, &undo_second);
  assert_eq!(g, before_second);
  assert!(g.node(at_home.node_id()).is_some());
}

#[test]
fn re_adding_a_formula_node_is_idempotent() {
  let atom = after("2026-10-01");
  let mut g = Graph::new();
  apply(&mut g, &[atom.node_added(0.0)]);
  apply(
    &mut g,
    &[Event::EdgeAdded {
      edge: eid(10),
      kind: base::EdgeKind::Dependency,
      from: nid(1),
      to: atom.node_id(),
    }],
  );
  let before = g.clone();

  // A replica, or a redo, adding the same atom again changes nothing, and
  // undoing that add must not delete the node either.
  let mut renamed = atom.node_added(0.0);
  if let Event::NodeAdded { name, .. } = &mut renamed {
    *name = "a different label".into();
  }
  let undo = apply(&mut g, &[renamed]);
  assert_eq!(g, before);
  assert!(undo.is_empty());
}

#[test]
fn formula_nodes_ignore_clicks() {
  let atom = after("2026-10-01");
  let mut g = Graph::new();

  // A stored bit set in the event is dropped: the atom decides.
  let mut added = atom.node_added(0.0);
  if let Event::NodeAdded {
    kind: NodeKind::Condition { satisfied, .. },
    ..
  } = &mut added
  {
    *satisfied = true;
  }
  apply(&mut g, &[added]);
  assert!(!g.is_satisfied(atom.node_id()));

  let set = Event::ConditionSet {
    node: atom.node_id(),
    satisfied: true,
  };
  let undo = apply(&mut g, &[set]);
  assert!(!g.is_satisfied(atom.node_id()));
  assert!(undo.is_empty());
}

// --- referents -----------------------------------------------------------

#[test]
fn referent_events_undo_exactly() {
  let (home, city) = (PlaceId::from_u128(1), PlaceId::from_u128(2));
  let cash = ResourceId::from_u128(3);
  let hours = ScheduleId::from_u128(4);
  let online = ContextId::from_u128(5);
  let define = vec![
    Event::PlaceDefined {
      place: city,
      name: "Chicago".into(),
      within: None,
    },
    Event::PlaceDefined {
      place: home,
      name: "Home".into(),
      within: Some(city),
    },
    Event::ResourceDefined {
      resource: cash,
      name: "Fun budget".into(),
      unit: Unit::Money {
        currency: "USD".into(),
        minor_digits: 2,
      },
      balance: 32_000,
    },
    Event::ScheduleDefined {
      schedule: hours,
      name: "Business hours".into(),
      spans: vec![Span::Weekly {
        days: WeekdaySet::WORKDAYS,
        start: tod("09:00"),
        end: tod("17:00"),
      }],
    },
    Event::ContextDefined {
      context: online,
      name: "Online".into(),
    },
  ];
  let edit = vec![
    Event::PlaceChanged {
      place: home,
      name: "New home".into(),
      within: None,
    },
    Event::ResourceBalanceSet {
      resource: cash,
      balance: 1_000,
    },
    Event::ResourceRenamed {
      resource: cash,
      name: "Cash".into(),
    },
    Event::ScheduleChanged {
      schedule: hours,
      name: "Hours".into(),
      spans: vec![],
    },
    Event::ContextRenamed {
      context: online,
      name: "Connected".into(),
    },
    // Redefining replaces; undo restores the earlier definition.
    Event::ContextDefined {
      context: online,
      name: "Wired".into(),
    },
  ];
  let remove = vec![
    Event::PlaceRemoved { place: home },
    Event::ResourceRemoved { resource: cash },
    Event::ScheduleRemoved { schedule: hours },
    Event::ContextRemoved { context: online },
  ];

  let mut g = Graph::new();
  let mut states = vec![g.clone()];
  let mut undos = vec![];
  for batch in [define, edit, remove] {
    undos.push(apply(&mut g, &batch));
    states.push(g.clone());
  }
  assert_eq!(g.place(city).unwrap().name, "Chicago");
  assert!(g.place(home).is_none());

  while let Some(undo) = undos.pop() {
    states.pop();
    apply(&mut g, &undo);
    assert_eq!(&g, states.last().unwrap());
  }
  assert_eq!(g, Graph::new());
}

#[test]
fn live_referent_edits_supersede_their_predecessor() {
  let cash = ResourceId::from_u128(1);
  let set = |balance| Event::ResourceBalanceSet {
    resource: cash,
    balance,
  };
  assert!(set(32).supersedes(&set(3)));
  assert!(!set(32).supersedes(&Event::ResourceBalanceSet {
    resource: ResourceId::from_u128(2),
    balance: 3,
  }));
}

// --- evaluation ----------------------------------------------------------

#[test]
fn after_and_before_flip_at_the_moment_in_the_facts_zone() {
  let g = Graph::new();
  let zone = chicago();
  let oct1 = at(&zone, "2026-10-01T09:00");
  let just_before = oct1 - SignedDuration::from_nanos(1);

  let t = after("2026-10-01T09:00").eval(&g, &facts(just_before, &zone));
  assert!(!t.holds);
  assert_eq!(t.until, Some(oct1));
  let t = after("2026-10-01T09:00").eval(&g, &facts(oct1, &zone));
  assert!(t.holds);
  assert_eq!(t.until, None);

  let before = Atom::Before {
    at: moment("2026-10-01T09:00"),
  };
  let t = before.eval(&g, &facts(just_before, &zone));
  assert!(t.holds);
  assert_eq!(t.until, Some(oct1));
  assert!(!before.eval(&g, &facts(oct1, &zone)).holds);

  // Floating: 9am is 9am in whichever zone the facts carry. At 8am in
  // Chicago it is already 10pm in Tokyo, and still 4am in Honolulu.
  let early = at(&zone, "2026-10-01T08:00");
  let tokyo = TimeZone::get("Asia/Tokyo").unwrap();
  let t = after("2026-10-01T09:00").eval(&g, &facts(early, &tokyo));
  assert_eq!((t.holds, t.until), (true, None));
  let honolulu = TimeZone::get("Pacific/Honolulu").unwrap();
  let t = after("2026-10-01T09:00").eval(&g, &facts(early, &honolulu));
  assert_eq!(t.until, Some(at(&honolulu, "2026-10-01T09:00")));
}

#[test]
fn weekly_windows_wrap_midnight() {
  let zone = chicago();
  let fri = WeekdaySet::from_iter([Weekday::Friday]);
  let (g, late) = weekly(fri, "22:00", "02:00");
  // 2 Oct 2026 is a Friday.
  let eval = |s| late.eval(&g, &facts(at(&zone, s), &zone));

  let t = eval("2026-10-02T21:00");
  assert!(!t.holds);
  assert_eq!(t.until, Some(at(&zone, "2026-10-02T22:00")));
  let t = eval("2026-10-03T01:00");
  assert!(t.holds);
  assert_eq!(t.until, Some(at(&zone, "2026-10-03T02:00")));
  // Saturday 22:00 is not a window: the next opens a week on.
  let t = eval("2026-10-03T22:00");
  assert!(!t.holds);
  assert_eq!(t.until, Some(at(&zone, "2026-10-09T22:00")));

  // Equal ends mean the whole day.
  let (g, all_fri) = weekly(fri, "00:00", "00:00");
  let t = all_fri.eval(&g, &facts(at(&zone, "2026-10-02T12:00"), &zone));
  assert!(t.holds);
  assert_eq!(t.until, Some(at(&zone, "2026-10-03T00:00")));
}

#[test]
fn weekly_windows_follow_dst() {
  let zone = chicago();
  let sun = WeekdaySet::from_iter([Weekday::Sunday]);

  // 8 Mar 2026: clocks jump from 02:00 CST to 03:00 CDT. A 01:00 – 04:00
  // window is two real hours long and closes at 04:00 CDT.
  let (g, early) = weekly(sun, "01:00", "04:00");
  let now = at(&zone, "2026-03-08T01:30");
  let t = early.eval(&g, &facts(now, &zone));
  assert!(t.holds);
  assert_eq!(t.until, Some("2026-03-08T09:00Z".parse().unwrap()));

  // A window opening inside the gap opens just after it.
  let (g, gap) = weekly(sun, "02:30", "05:00");
  let t = gap.eval(&g, &facts(now, &zone));
  assert!(!t.holds);
  assert_eq!(t.until, Some("2026-03-08T08:30Z".parse().unwrap()));

  // 1 Nov 2026: 01:00 – 02:00 happens twice. A window closing at 01:30
  // closes the first time round, in CDT.
  let (g, fold) = weekly(sun, "00:30", "01:30");
  let now = at(&zone, "2026-11-01T00:45");
  let t = fold.eval(&g, &facts(now, &zone));
  assert!(t.holds);
  assert_eq!(t.until, Some("2026-11-01T06:30Z".parse().unwrap()));
}

#[test]
fn once_spans_and_adjacent_windows_merge() {
  let zone = TimeZone::UTC;
  let schedule = ScheduleId::from_u128(1);
  let mut g = Graph::new();
  apply(
    &mut g,
    &[Event::ScheduleDefined {
      schedule,
      name: "Conference".into(),
      spans: vec![
        Span::Once {
          start: moment("2027-05-03T09:00"),
          end: moment("2027-05-04T00:00"),
        },
        Span::Once {
          start: moment("2027-05-04T00:00"),
          end: moment("2027-05-05T17:00"),
        },
      ],
    }],
  );
  let atom = Atom::Within { schedule };

  // A one-off window a year out is still found: nothing repeats.
  let t = atom.eval(&g, &facts(at(&zone, "2026-05-01T00:00"), &zone));
  assert!(!t.holds);
  assert_eq!(t.until, Some(at(&zone, "2027-05-03T09:00")));

  // Back-to-back windows are one: no flip at midnight between them.
  let t = atom.eval(&g, &facts(at(&zone, "2027-05-03T12:00"), &zone));
  assert!(t.holds);
  assert_eq!(t.until, Some(at(&zone, "2027-05-05T17:00")));

  let t = atom.eval(&g, &facts(at(&zone, "2027-06-01T00:00"), &zone));
  assert_eq!((t.holds, t.until), (false, None));
}

#[test]
fn places_nest_upward() {
  let (errands, hardware, home) = (
    PlaceId::from_u128(1),
    PlaceId::from_u128(2),
    PlaceId::from_u128(3),
  );
  let mut g = Graph::new();
  apply(
    &mut g,
    &[
      Event::PlaceDefined {
        place: errands,
        name: "Errands".into(),
        within: None,
      },
      Event::PlaceDefined {
        place: hardware,
        name: "Hardware store".into(),
        within: Some(errands),
      },
      Event::PlaceDefined {
        place: home,
        name: "Home".into(),
        within: None,
      },
    ],
  );
  let mut f = facts(Timestamp::UNIX_EPOCH, &TimeZone::UTC);
  f.places.insert(hardware);

  let t = Atom::At { place: errands }.eval(&g, &f);
  assert!(t.holds);
  assert_eq!(
    t.why,
    Explanation::Place {
      here: vec![hardware],
    }
  );
  assert!(Atom::At { place: hardware }.eval(&g, &f).holds);
  assert!(!Atom::At { place: home }.eval(&g, &f).holds);

  // A `within` loop neither hangs nor invents membership.
  apply(
    &mut g,
    &[Event::PlaceChanged {
      place: errands,
      name: "Errands".into(),
      within: Some(hardware),
    }],
  );
  assert!(!Atom::At { place: home }.eval(&g, &f).holds);
}

#[test]
fn resources_contexts_and_missing_referents() {
  let cash = ResourceId::from_u128(1);
  let online = ContextId::from_u128(2);
  let mut g = Graph::new();
  apply(
    &mut g,
    &[
      Event::ResourceDefined {
        resource: cash,
        name: "Cash".into(),
        unit: Unit::Money {
          currency: "USD".into(),
          minor_digits: 2,
        },
        balance: 3_210,
      },
      Event::ContextDefined {
        context: online,
        name: "Online".into(),
      },
    ],
  );
  let mut f = facts(Timestamp::UNIX_EPOCH, &TimeZone::UTC);
  let needs = |at_least| Atom::Has {
    resource: cash,
    at_least,
  };

  let t = needs(5_000).eval(&g, &f);
  assert_eq!((t.holds, t.until), (false, None));
  assert_eq!(t.why, Explanation::Balance { have: 3_210 });
  assert!(needs(3_210).eval(&g, &f).holds);

  let in_online = Atom::In { context: online };
  assert!(!in_online.eval(&g, &f).holds);
  f.contexts.insert(online);
  assert!(in_online.eval(&g, &f).holds);

  let missing = [
    Atom::Has {
      resource: ResourceId::from_u128(9),
      at_least: 0,
    },
    Atom::In {
      context: ContextId::from_u128(9),
    },
    Atom::At {
      place: PlaceId::from_u128(9),
    },
    Atom::Within {
      schedule: ScheduleId::from_u128(9),
    },
  ];
  for atom in missing {
    let t = atom.eval(&g, &f);
    assert_eq!(
      (t.holds, t.until, t.why),
      (false, None, Explanation::Missing)
    );
  }
}

#[test]
fn free_time_counts_down_to_free_until() {
  let g = Graph::new();
  let zone = TimeZone::UTC;
  let now = at(&zone, "2026-10-01T14:00");
  let mut f = facts(now, &zone);
  let hour = Atom::Free { at_least: 60 };

  let t = hour.eval(&g, &f);
  assert_eq!((t.holds, t.why), (false, Explanation::Free { left: None }));

  f.free_until = Some(at(&zone, "2026-10-01T15:30"));
  let t = hour.eval(&g, &f);
  assert!(t.holds);
  assert_eq!(t.until, Some(at(&zone, "2026-10-01T14:30")));
  assert_eq!(t.why, Explanation::Free { left: Some(90) });

  f.now = at(&zone, "2026-10-01T14:30");
  let t = hour.eval(&g, &f);
  assert_eq!((t.holds, t.until), (false, None));

  // No minimum: free until free time ends.
  f.now = at(&zone, "2026-10-01T15:30");
  assert!(!Atom::Free { at_least: 0 }.eval(&g, &f).holds);
}

// --- derivation ------------------------------------------------------------

#[test]
fn formula_conditions_gate_readiness_and_set_the_horizon() {
  let zone = chicago();
  let opens = after("2026-10-01T09:00");
  let closes = Atom::Before {
    at: moment("2026-10-15T00:00"),
  };
  let mut g = Graph::new();
  add_task(&mut g, 1);
  add_task(&mut g, 2);
  let events = opens.require(&g, nid(1), eid(10));
  apply(&mut g, &events);
  let events = closes.require(&g, nid(1), eid(11));
  apply(&mut g, &events);
  let events = closes.require(&g, nid(2), eid(12));
  apply(&mut g, &events);

  let d = Derived::compute(&g, &facts(at(&zone, "2026-09-28T12:00"), &zone));
  assert_eq!(d.state(nid(1)), Some(NodeState::Blocked));
  assert_eq!(d.state(nid(2)), Some(NodeState::Ready));
  assert_eq!(d.state(opens.node_id()), Some(NodeState::Pending));
  assert_eq!(d.state(closes.node_id()), Some(NodeState::Satisfied));
  assert!(d.is_satisfied(closes.node_id()));
  assert!(!d.is_ready(opens.node_id()));
  assert_eq!(d.horizon(), Some(at(&zone, "2026-10-01T09:00")));
  assert_eq!(d.truth(opens.node_id()).unwrap().why, Explanation::Clock);

  let d = Derived::compute(&g, &facts(at(&zone, "2026-10-01T09:00"), &zone));
  assert_eq!(d.state(nid(1)), Some(NodeState::Ready));
  assert_eq!(d.horizon(), Some(at(&zone, "2026-10-15T00:00")));

  let d = Derived::compute(&g, &facts(at(&zone, "2026-10-15T00:00"), &zone));
  assert_eq!(d.state(nid(1)), Some(NodeState::Blocked));
  assert_eq!(d.state(nid(2)), Some(NodeState::Blocked));
  assert_eq!(d.horizon(), None);
}

#[test]
fn formula_conditions_are_sinks() {
  let atom = after("2026-10-01");
  let f = atom.node_id();
  let mut g = Graph::new();
  add_task(&mut g, 1);
  let events = atom.require(&g, nid(1), eid(10));
  apply(&mut g, &events);
  // An edge out of a formula node, which the UI never offers, would close
  // a loop; derivation ignores it.
  apply(
    &mut g,
    &[Event::EdgeAdded {
      edge: eid(11),
      kind: base::EdgeKind::Dependency,
      from: f,
      to: nid(1),
    }],
  );
  assert!(cyclic_nodes(&g).is_empty());
  assert!(base::cycle_peers(&g, nid(1)).is_empty());

  let zone = TimeZone::UTC;
  let d = Derived::compute(&g, &facts(at(&zone, "2026-10-02T00:00"), &zone));
  assert_eq!(d.state(f), Some(NodeState::Satisfied));
  assert_eq!(d.state(nid(1)), Some(NodeState::Ready));
}

// --- property tests -------------------------------------------------------

mod props {
  use proptest::prelude::*;

  use super::*;

  fn arb_moment() -> impl Strategy<Value = Moment> {
    (-9999i16..=9999, 1i8..=12, 1i8..=28, 0i8..24, 0i8..60).prop_map(
      |(y, mo, d, h, mi)| Moment::new(datetime(y, mo, d, h, mi, 0, 0)),
    )
  }

  /// Moments around now, where zones and DST are interesting.
  fn arb_near_moment() -> impl Strategy<Value = Moment> {
    (1990i16..2060, 1i8..=12, 1i8..=28, 0i8..24, 0i8..60).prop_map(
      |(y, mo, d, h, mi)| Moment::new(datetime(y, mo, d, h, mi, 0, 0)),
    )
  }

  fn arb_now() -> impl Strategy<Value = Timestamp> {
    // 1990 – 2060, at nanosecond precision.
    (631_152_000i64..2_871_763_200, 0i32..1_000_000_000)
      .prop_map(|(s, ns)| Timestamp::new(s, ns).unwrap())
  }

  fn arb_zone() -> impl Strategy<Value = TimeZone> {
    prop_oneof![
      Just(TimeZone::UTC),
      Just(chicago()),
      // Half-hour DST shifts.
      Just(TimeZone::get("Australia/Lord_Howe").unwrap()),
      Just(TimeZone::get("Asia/Kolkata").unwrap()),
    ]
  }

  /// Atoms over small id and amount ranges, so equal atoms are common.
  fn arb_atom() -> impl Strategy<Value = Atom> {
    let id = 0u128..4;
    prop_oneof![
      arb_moment().prop_map(|at| Atom::After { at }),
      arb_moment().prop_map(|at| Atom::Before { at }),
      id.clone().prop_map(|n| Atom::Within {
        schedule: ScheduleId::from_u128(n),
      }),
      (0u32..4).prop_map(|at_least| Atom::Free { at_least }),
      id.clone().prop_map(|n| Atom::At {
        place: PlaceId::from_u128(n),
      }),
      (id.clone(), -2i64..2).prop_map(|(n, at_least)| Atom::Has {
        resource: ResourceId::from_u128(n),
        at_least,
      }),
      id.prop_map(|n| Atom::In {
        context: ContextId::from_u128(n),
      }),
    ]
  }

  fn arb_weekly() -> impl Strategy<Value = Span> {
    (0u8..128, 0i8..24, 0i8..60, 0i8..24, 0i8..60).prop_map(
      |(days, sh, sm, eh, em)| Span::Weekly {
        days: (0..7)
          .filter(|i| days & (1 << i) != 0)
          .map(|i| Weekday::from_monday_zero_offset(i).unwrap())
          .collect(),
        start: TimeOfDay::new(sh, sm).unwrap(),
        end: TimeOfDay::new(eh, em).unwrap(),
      },
    )
  }

  fn arb_span() -> impl Strategy<Value = Span> {
    prop_oneof![
      arb_weekly(),
      (arb_near_moment(), 0i64..20_000).prop_map(|(start, mins)| {
        let end = start.civil() + SignedDuration::from_mins(mins);
        Span::Once {
          start,
          end: Moment::new(end),
        }
      }),
    ]
  }

  /// Whether `atom` holds at `now`.
  fn holds(atom: &Atom, g: &Graph, zone: &TimeZone, now: Timestamp) -> bool {
    atom.eval(g, &facts(now, zone)).holds
  }

  /// Truth is constant from `now` up to `until`, and differs at `until`.
  fn check_until(
    atom: &Atom,
    g: &Graph,
    zone: &TimeZone,
    now: Timestamp,
    samples: &[f64],
  ) -> Result<(), TestCaseError> {
    let t = atom.eval(g, &facts(now, zone));
    let Some(until) = t.until else {
      return Ok(());
    };
    prop_assert!(until > now);
    let span = until.duration_since(now);
    for s in samples {
      let at = now + span.mul_f64(*s).min(span - SignedDuration::from_nanos(1));
      prop_assert_eq!(holds(atom, g, zone, at), t.holds, "at {}", at);
    }
    // Exact, except when a schedule stays open through its look-ahead.
    let look_ahead = now + SignedDuration::from_hours(24 * 8);
    if until < look_ahead {
      prop_assert_ne!(holds(atom, g, zone, until), t.holds, "at {}", until);
    }
    Ok(())
  }

  proptest! {
    /// Literals print in the one spelling they parse from.
    #[test]
    fn moments_round_trip(m in arb_moment()) {
      let json = serde_json::to_string(&m).unwrap();
      prop_assert_eq!(serde_json::from_str::<Moment>(&json).unwrap(), m);
    }

    /// Distinct atoms encode, and hash, to distinct ids; equal atoms to
    /// equal ones.
    #[test]
    fn node_ids_are_injective(a in arb_atom(), b in arb_atom()) {
      prop_assert_eq!(a == b, a.canonical_bytes() == b.canonical_bytes());
      prop_assert_eq!(a == b, a.node_id() == b.node_id());
      prop_assert!(a.node_id().is_value_addressed());
    }

    /// However requirements are added, each atom is one node, with one
    /// edge per requirement.
    #[test]
    fn equal_atoms_share_one_node(
      reqs in prop::collection::vec((0u128..4, arb_atom()), 0..24)
    ) {
      let mut g = Graph::new();
      for n in 0..4 {
        add_task(&mut g, n + 100);
      }
      for (k, (task, atom)) in reqs.iter().enumerate() {
        let events = atom.require(&g, nid(task + 100), eid(k as u128));
        apply(&mut g, &events);
      }
      let distinct: HashSet<&Atom> = reqs.iter().map(|(_, a)| a).collect();
      let formula = g.nodes().filter(|n| n.kind.atom().is_some()).count();
      prop_assert_eq!(formula, distinct.len());
      prop_assert_eq!(g.edges().count(), reqs.len());
      for atom in distinct {
        prop_assert_eq!(g.node(atom.node_id()).unwrap().kind.atom(), Some(atom));
      }
    }

    /// `After` and `Before` flip exactly at `until`, in any zone.
    #[test]
    fn clock_atoms_flip_at_until(
      m in arb_near_moment(),
      now in arb_now(),
      zone in arb_zone(),
      samples in prop::collection::vec(0.0f64..1.0, 3),
    ) {
      let g = Graph::new();
      for atom in [Atom::After { at: m }, Atom::Before { at: m }] {
        check_until(&atom, &g, &zone, now, &samples)?;
        // Once past the moment, it never flips back.
        if atom.eval(&g, &facts(now, &zone)).until.is_none() {
          let later = now + SignedDuration::from_hours(24 * 365);
          prop_assert_eq!(
            holds(&atom, &g, &zone, later),
            holds(&atom, &g, &zone, now)
          );
        }
      }
    }

    /// A schedule's truth holds still until `until` and flips there,
    /// across midnight wraps and DST in either direction.
    #[test]
    fn schedules_flip_at_until(
      spans in prop::collection::vec(arb_span(), 0..4),
      now in arb_now(),
      zone in arb_zone(),
      samples in prop::collection::vec(0.0f64..1.0, 4),
    ) {
      let schedule = ScheduleId::from_u128(1);
      let mut g = Graph::new();
      apply(&mut g, &[Event::ScheduleDefined {
        schedule,
        name: "s".into(),
        spans,
      }]);
      check_until(&Atom::Within { schedule }, &g, &zone, now, &samples)?;
    }

    /// A graph of tasks requiring formula atoms, with arbitrary extra
    /// edges, even out of formula nodes: the horizon is the earliest
    /// `until`, formula conditions are never Ready or Cyclic, and a task
    /// is Ready exactly when its requirements are satisfied.
    #[test]
    fn derivation_with_formulas(
      tasks in 1usize..8,
      reqs in prop::collection::vec((0usize..8, arb_atom()), 0..12),
      edges in prop::collection::vec((0usize..20, 0usize..20), 0..16),
      now in arb_now(),
      zone in arb_zone(),
    ) {
      let mut g = Graph::new();
      for n in 0..tasks {
        add_task(&mut g, n as u128);
      }
      for (k, (task, atom)) in reqs.iter().enumerate() {
        let from = nid((task % tasks) as u128);
        let events = atom.require(&g, from, eid(k as u128));
        apply(&mut g, &events);
      }
      let ids: Vec<NodeId> = g.nodes().map(|n| n.id).collect();
      for (k, (a, b)) in edges.iter().enumerate() {
        apply(&mut g, &[Event::EdgeAdded {
          edge: eid(1_000 + k as u128),
          kind: base::EdgeKind::Dependency,
          from: ids[a % ids.len()],
          to:   ids[b % ids.len()],
        }]);
      }

      let f = facts(now, &zone);
      let d = Derived::compute(&g, &f);
      let cyclic = cyclic_nodes(&g);
      let mut untils = vec![];
      for node in g.nodes() {
        if let Some(atom) = node.kind.atom() {
          let t = atom.eval(&g, &f);
          untils.extend(t.until);
          prop_assert_eq!(d.truth(node.id), Some(&t));
          prop_assert_eq!(d.is_satisfied(node.id), t.holds);
          prop_assert!(!d.is_ready(node.id));
          prop_assert!(!cyclic.contains(&node.id));
          prop_assert_ne!(d.state(node.id), Some(NodeState::Cyclic));
        } else {
          let met = g.requirements_of(node.id).all(|e| d.is_satisfied(e.to));
          prop_assert_eq!(
            d.is_ready(node.id),
            met && !cyclic.contains(&node.id)
          );
        }
      }
      prop_assert_eq!(d.horizon(), untils.into_iter().min());
    }
  }
}

mod group_props {
  use base::apply_group;
  use proptest::prelude::*;

  use super::*;

  proptest! {
    /// Whatever a gesture removes, it leaves no formula node it unlinked
    /// without dependents, and its inverse restores the graph exactly.
    #[test]
    fn groups_remove_orphans_and_undo_exactly(
      reqs in prop::collection::vec((0u128..4, 0u128..3), 1..12),
      removals in prop::collection::vec((any::<bool>(), 0usize..16), 1..4),
    ) {
      let mut g = Graph::new();
      for n in 0..4 {
        add_task(&mut g, n);
      }
      let atom = |n| Atom::At { place: PlaceId::from_u128(n) };
      for (k, (task, a)) in reqs.iter().enumerate() {
        let events = atom(*a).require(&g, nid(*task), eid(k as u128));
        apply(&mut g, &events);
      }
      let gesture: Vec<Event> = removals
        .iter()
        .map(|(node, k)| match node {
          true => Event::NodeRemoved { node: nid((*k % 4) as u128) },
          false => Event::EdgeRemoved { edge: eid((*k % reqs.len()) as u128) },
        })
        .collect();

      let before = g.clone();
      let (applied, undo) = apply_group(&mut g, gesture.clone());
      prop_assert_eq!(&applied[..gesture.len()], &gesture[..]);
      for node in g.nodes().filter(|n| n.kind.atom().is_some()) {
        prop_assert!(g.dependents_of(node.id).next().is_some());
      }
      apply_batch(&mut g, &undo);
      prop_assert_eq!(&g, &before);
    }
  }
}

#[test]
fn pruning_removes_orphans_and_unused_referents_and_undoes() {
  let (errands, store, home, city) = (
    PlaceId::from_u128(1),
    PlaceId::from_u128(2),
    PlaceId::from_u128(3),
    PlaceId::from_u128(4),
  );
  let (cash, spare) = (ResourceId::from_u128(5), ResourceId::from_u128(6));
  let hours = ScheduleId::from_u128(7);
  let (online, phone) = (ContextId::from_u128(8), ContextId::from_u128(9));
  let place = |place, name: &str, within| Event::PlaceDefined {
    place,
    name: name.into(),
    within,
  };
  let resource = |resource, name: &str| Event::ResourceDefined {
    resource,
    name: name.into(),
    unit: Unit::Minutes,
    balance: 0,
  };
  let context = |context, name: &str| Event::ContextDefined {
    context,
    name: name.into(),
  };
  let mut g = Graph::new();
  apply(
    &mut g,
    &[
      place(errands, "Errands", None),
      place(store, "Hardware store", Some(errands)),
      place(city, "Chicago", None),
      place(home, "Home", Some(city)),
      resource(cash, "Cash"),
      resource(spare, "Spare"),
      Event::ScheduleDefined {
        schedule: hours,
        name: "Hours".into(),
        spans: vec![],
      },
      context(online, "Online"),
      context(phone, "Phone"),
    ],
  );
  add_task(&mut g, 1);
  // Required: At(Errands), Has(Cash). Standing alone: In(Phone), After.
  let required = [
    Atom::At { place: errands },
    Atom::Has {
      resource: cash,
      at_least: 30,
    },
  ];
  for (i, atom) in required.iter().enumerate() {
    apply(
      &mut g,
      &[
        atom.node_added(0.0),
        Event::EdgeAdded {
          edge: eid(i as u128 + 1),
          kind: base::EdgeKind::Dependency,
          from: nid(1),
          to: atom.node_id(),
        },
      ],
    );
  }
  let lonely = [Atom::In { context: phone }, after("2026-10-01")];
  for atom in &lonely {
    apply(&mut g, &[atom.node_added(0.0)]);
  }
  let before = g.clone();

  // Online is active, so it stays although no atom uses it.
  let mut now = facts(Timestamp::UNIX_EPOCH, &chicago());
  now.contexts.insert(online);
  let events = base::prune(&g, &now);
  let mut orphans: Vec<NodeId> = lonely.iter().map(Atom::node_id).collect();
  orphans.sort();
  let mut expected: Vec<Event> = orphans
    .into_iter()
    .map(|node| Event::NodeRemoved { node })
    .collect();
  expected.extend([
    Event::PlaceRemoved { place: home },
    Event::PlaceRemoved { place: city },
    Event::ResourceRemoved { resource: spare },
    Event::ScheduleRemoved { schedule: hours },
    Event::ContextRemoved { context: phone },
  ]);
  assert_eq!(events, expected);

  let (_, inverse) = base::apply_group(&mut g, events);
  assert!(g.place(errands).is_some() && g.place(store).is_some());
  assert!(g.resource(cash).is_some() && g.context(online).is_some());
  assert!(base::prune(&g, &now).is_empty(), "pruning is idempotent");
  apply(&mut g, &inverse);
  assert_eq!(g, before);

  // Standing at Home keeps Home and the city it lies in.
  let mut at_home = facts(Timestamp::UNIX_EPOCH, &chicago());
  at_home.places.insert(home);
  assert!(
    !base::prune(&before, &at_home)
      .iter()
      .any(|e| matches!(e, Event::PlaceRemoved { .. }))
  );
}
