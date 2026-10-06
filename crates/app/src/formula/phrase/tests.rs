use base::{
  ContextId, Event, Graph, PlaceId, ResourceId, ScheduleId, Unit, apply_batch,
};
use jiff::civil::{date, datetime};

use super::*;

/// Monday 28 September 2026.
const TODAY: Date = date(2026, 9, 28);

fn graph(events: &[Event]) -> Graph {
  let mut g = Graph::new();
  apply_batch(&mut g, events);
  g
}

fn atoms(text: &str, g: &Graph) -> Vec<Atom> {
  offers(text, g, TODAY).into_iter().map(|o| o.atom).collect()
}

fn at(y: i16, mo: i8, d: i8, h: i8, mi: i8) -> Moment {
  Moment::new(datetime(y, mo, d, h, mi, 0, 0))
}

#[test]
fn dates_and_times() {
  let g = Graph::new();
  let after = |text| match atoms(text, &g).first() {
    Some(Atom::After { at }) => Some(*at),
    _ => None,
  };
  assert_eq!(after("after oct 1"), Some(at(2026, 10, 1, 0, 0)));
  assert_eq!(after("from 1st October"), Some(at(2026, 10, 1, 0, 0)));
  assert_eq!(after("starting 2026-10-01"), Some(at(2026, 10, 1, 0, 0)));
  assert_eq!(after("after oct 1 2027 9am"), Some(at(2027, 10, 1, 9, 0)));
  // A day already past this year means next year's.
  assert_eq!(after("after sep 1"), Some(at(2027, 9, 1, 0, 0)));
  assert_eq!(after("after today"), Some(at(2026, 9, 28, 0, 0)));
  assert_eq!(
    after("after tomorrow at 5:30pm"),
    Some(at(2026, 9, 29, 17, 30))
  );
  // Weekdays count from today, today included.
  assert_eq!(after("from friday"), Some(at(2026, 10, 2, 0, 0)));
  assert_eq!(after("from mon"), Some(at(2026, 9, 28, 0, 0)));
  // A time alone is today.
  assert_eq!(after("after 5 pm"), Some(at(2026, 9, 28, 17, 0)));
  assert_eq!(after("after noon"), Some(at(2026, 9, 28, 12, 0)));
  assert_eq!(after("after 17:00"), Some(at(2026, 9, 28, 17, 0)));

  assert!(matches!(
    atoms("until sunday", &g)[..],
    [Atom::Before { at }, ..] if at == at_midnight(2026, 10, 4)
  ));
  assert_eq!(after("after 13pm"), None);
  assert_eq!(after("after octopus"), None);
  assert_eq!(after("after"), None);
}

fn at_midnight(y: i16, mo: i8, d: i8) -> Moment {
  at(y, mo, d, 0, 0)
}

#[test]
fn free_time() {
  let g = Graph::new();
  let free = |text| {
    atoms(text, &g).into_iter().find_map(|a| match a {
      Atom::Free { at_least } => Some(at_least),
      _ => None,
    })
  };
  assert_eq!(free("1h free"), Some(60));
  assert_eq!(free("30m"), Some(30));
  assert_eq!(free("free for 1 h 30 min"), Some(90));
  assert_eq!(free("1.5 hours"), Some(90));
  assert_eq!(free("45 minutes free"), Some(45));
  assert_eq!(free("45"), None, "a bare number is not a duration");
  assert_eq!(free("0m"), None);
}

#[test]
fn places_match_by_name_or_are_offered_new() {
  let home = PlaceId::from_u128(1);
  let g = graph(&[Event::PlaceDefined {
    place: home,
    name: "Home".into(),
    within: None,
  }]);
  let found = offers("at home", &g, TODAY);
  assert_eq!(found[0].atom, Atom::At { place: home });
  assert!(found[0].define.is_empty());
  assert_eq!(found[0].label, "At Home");
  assert!(
    found.iter().all(|o| o.define.is_empty()),
    "an exact name offers no new place"
  );

  let new = offers("@hardware store", &g, TODAY);
  let Some(offer) = new.iter().find(|o| !o.define.is_empty()) else {
    panic!("no new place offered: {new:?}");
  };
  assert_eq!(offer.label, "At Hardware store (new place)");
  let [Event::PlaceDefined { place, name, .. }] = &offer.define[..] else {
    panic!("{:?}", offer.define);
  };
  assert_eq!(name, "Hardware store");
  assert_eq!(offer.atom, Atom::At { place: *place });
}

#[test]
fn schedules() {
  let hours = ScheduleId::from_u128(1);
  let g = graph(&[Event::ScheduleDefined {
    schedule: hours,
    name: "Business hours".into(),
    spans: vec![],
  }]);
  assert_eq!(
    atoms("during business hours", &g)[0],
    Atom::Within { schedule: hours }
  );
  assert_eq!(atoms("business", &g)[0], Atom::Within { schedule: hours });

  // The two everyone agrees on are made with their windows.
  let weekends = offers("on weekends", &g, TODAY);
  let [Event::ScheduleDefined { name, spans, .. }] = &weekends[0].define[..]
  else {
    panic!("{weekends:?}");
  };
  assert_eq!(name, "Weekends");
  assert_eq!(spans.len(), 1);

  // Only "during" makes up a schedule from any text.
  let made = offers("during tax season", &g, TODAY);
  assert!(
    made
      .iter()
      .any(|o| o.label == "During Tax season (new schedule)")
  );
  assert!(
    offers("tax season", &g, TODAY)
      .iter()
      .all(|o| !matches!(o.atom, Atom::Within { .. }))
  );
}

#[test]
fn resources_by_unit_and_amount() {
  let (fun, cells) = (ResourceId::from_u128(1), ResourceId::from_u128(2));
  let g = graph(&[
    Event::ResourceDefined {
      resource: fun,
      name: "Fun budget".into(),
      unit: Unit::Money {
        currency: "USD".into(),
        minor_digits: 2,
      },
      balance: 0,
    },
    Event::ResourceDefined {
      resource: cells,
      name: "Batteries".into(),
      unit: Unit::Count {
        noun: "battery".into(),
      },
      balance: 0,
    },
  ]);
  let first = |text| offers(text, &g, TODAY).into_iter().next();

  let fifty = first("$50").unwrap();
  assert_eq!(
    fifty.atom,
    Atom::Has {
      resource: fun,
      at_least: 5_000,
    }
  );
  assert_eq!(fifty.label, "Has $50.00 in Fun budget");
  assert_eq!(
    first("have $12.5").unwrap().atom,
    Atom::Has {
      resource: fun,
      at_least: 1_250,
    }
  );
  assert!(first("$1.234").is_none_or(|o| !matches!(o.atom, Atom::Has { .. })));

  let three = first("3 batteries").unwrap();
  assert_eq!(
    three.atom,
    Atom::Has {
      resource: cells,
      at_least: 3,
    }
  );
  assert_eq!(three.label, "Has 3 batteries");

  // Money in a currency no resource holds, and a noun nothing counts,
  // offer a new resource.
  let euros = first("€20").unwrap();
  assert_eq!(euros.label, "Has €20.00 in Money (new resource)");
  let boxes = first("2 boxes").unwrap();
  assert_eq!(boxes.label, "Has 2 boxes (new resource)");
  let [Event::ResourceDefined { unit, name, .. }] = &boxes.define[..] else {
    panic!("{boxes:?}");
  };
  assert_eq!(unit, &Unit::Count { noun: "box".into() });
  assert_eq!(name, "Boxes");
}

#[test]
fn contexts_match_any_phrase_and_with_makes_one() {
  let online = ContextId::from_u128(1);
  let g = graph(&[Event::ContextDefined {
    context: online,
    name: "Online".into(),
  }]);
  let found = offers("online", &g, TODAY);
  assert_eq!(found.len(), 1);
  assert_eq!(found[0].atom, Atom::In { context: online });

  // Unmatched text makes no context, or every search would offer one.
  assert!(offers("buy milk", &g, TODAY).is_empty());
  let sam = offers("with sam", &g, TODAY);
  assert_eq!(sam.last().unwrap().label, "With sam (new context)");
}

#[test]
fn nothing_for_nothing() {
  assert!(offers("   ", &Graph::new(), TODAY).is_empty());
}

#[test]
fn schedule_windows() {
  use base::{Span, WeekdaySet};
  let tod = |s: &str| s.parse::<TimeOfDay>().unwrap();
  let weekly = |days, start, end| {
    Some(Span::Weekly {
      days,
      start: tod(start),
      end: tod(end),
    })
  };
  assert_eq!(
    span("weekdays 9:00-17:00", TODAY),
    weekly(WeekdaySet::WORKDAYS, "09:00", "17:00")
  );
  assert_eq!(
    span("Sat, Sun 10am - 4pm", TODAY),
    weekly(WeekdaySet::WEEKEND, "10:00", "16:00")
  );
  let fri_to_mon = [
    Weekday::Friday,
    Weekday::Saturday,
    Weekday::Sunday,
    Weekday::Monday,
  ]
  .into_iter()
  .collect();
  assert_eq!(
    span("fri-mon 22:00-02:00", TODAY),
    weekly(fri_to_mon, "22:00", "02:00")
  );
  // A date makes a one-off window; one past midnight ends the next day.
  assert_eq!(
    span("dec 24 9am-12pm", TODAY),
    Some(Span::Once {
      start: at(2026, 12, 24, 9, 0),
      end: at(2026, 12, 24, 12, 0),
    })
  );
  assert_eq!(
    span("dec 31 22:00-01:00", TODAY),
    Some(Span::Once {
      start: at(2026, 12, 31, 22, 0),
      end: at(2027, 1, 1, 1, 0),
    })
  );
  assert_eq!(span("weekdays", TODAY), None);
  assert_eq!(span("someday 9:00-10:00", TODAY), None);
}

#[test]
fn balances_in_a_unit() {
  let usd = Unit::Money {
    currency: "USD".into(),
    minor_digits: 2,
  };
  assert_eq!(amount("320", &usd), Some(32_000));
  assert_eq!(amount("$ 12.5", &usd), Some(1_250));
  assert_eq!(amount("USD 3", &usd), Some(300));
  assert_eq!(amount("-4.10", &usd), Some(-410));
  assert_eq!(amount("12.345", &usd), None);
  assert_eq!(amount("2h 30m", &Unit::Minutes), Some(150));
  assert_eq!(amount("90", &Unit::Minutes), Some(90));
  let count = Unit::Count {
    noun: "battery".into(),
  };
  assert_eq!(amount("3", &count), Some(3));
  assert_eq!(amount("three", &count), None);
}

#[test]
fn free_time_ends() {
  use jiff::tz::TimeZone;
  let zone = TimeZone::UTC;
  let now: jiff::Timestamp = "2026-09-28T12:00Z".parse().unwrap();
  let until = |s| until(s, now, &zone).map(|t| t.to_string());
  assert_eq!(until("15:30"), Some("2026-09-28T15:30:00Z".into()));
  assert_eq!(until("3pm"), Some("2026-09-28T15:00:00Z".into()));
  // A time already past today is tomorrow's.
  assert_eq!(until("9am"), Some("2026-09-29T09:00:00Z".into()));
  assert_eq!(until("1h 30m"), Some("2026-09-28T13:30:00Z".into()));
  assert_eq!(until("soon"), None);
}
