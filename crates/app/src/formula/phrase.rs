//! Reading a requirement typed as a phrase: "after oct 1", "at home", "$50",
//! "1h free", "online" (plans/formula-conditions.md, "Adding").
//!
//! The grammar is small and forgiving: a handful of lead words, dates,
//! times, durations and amounts. Each reading becomes an [`Offer`]: the
//! atom, and the referent to define first when the phrase names a place,
//! schedule, resource or context that does not exist yet. Names are matched
//! against the referents already defined, so "at home" finds Home.

use base::{
  Amount, Atom, ContextId, Event, Graph, Minutes, Moment, PlaceId, Resource,
  ResourceId, ScheduleId, Span, TimeOfDay, Unit, WeekdaySet,
};
use jiff::{
  Timestamp,
  civil::{Date, Time, Weekday},
  tz::TimeZone,
};

use super::describe::{atom_label, currency_symbol, plural};
use crate::query;

/// One way to read the typed phrase as a requirement.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
  /// The formula the requirement is on.
  pub atom:   Atom,
  /// Referents to define before requiring the atom: a new place, say.
  /// Empty when everything the atom points at exists.
  pub define: Vec<Event>,
  /// What the offer says: "At Home", "At Hardware store (new place)".
  pub label:  String,
}

/// Every reading of `text` as a formula requirement, best first. `today`
/// anchors relative dates ("friday", "oct 1" without a year).
pub fn offers(text: &str, graph: &Graph, today: Date) -> Vec<Offer> {
  let text = text.trim().to_lowercase();
  let words: Vec<&str> = text.split_whitespace().collect();
  if words.is_empty() {
    return vec![];
  }
  let mut out = Vec::new();
  let mut push = |offer: Offer| {
    if !out.iter().any(|o: &Offer| o.atom == offer.atom) {
      out.push(offer);
    }
  };
  let existing = |atom: Atom| Offer {
    label: atom_label(graph, &atom, today),
    atom,
    define: vec![],
  };

  // Timing.
  match words[0] {
    "after" | "from" | "starting" | "starts" => {
      if let Some(at) = moment(&words[1..], today) {
        push(existing(Atom::After { at }));
      }
    }
    "before" | "until" | "till" => {
      if let Some(at) = moment(&words[1..], today) {
        push(existing(Atom::Before { at }));
      }
    }
    _ => {}
  }
  if let Some(at_least) = free(&words) {
    push(existing(Atom::Free { at_least }));
  }
  for offer in schedules(&words, graph, today) {
    push(offer);
  }

  // Location.
  let place = match words[0] {
    "at" => Some(words[1..].join(" ")),
    w if w.starts_with('@') => Some(text[1..].trim().to_string()),
    _ => None,
  };
  if let Some(name) = place.filter(|n| !n.is_empty()) {
    for offer in named(
      &name,
      graph.places().map(|p| (p.name.as_str(), p.id)),
      |place| Atom::At { place },
      |name| {
        let place = PlaceId::new();
        (place, Event::PlaceDefined {
          place,
          name,
          within: None,
        })
      },
      "place",
      graph,
      today,
    ) {
      push(offer);
    }
  }

  // Resources.
  let has = match words[0] {
    "have" | "has" | "with" => &words[1..],
    _ => &words[..],
  };
  for offer in resources(has, graph, today) {
    push(offer);
  }

  // Contexts: any phrase can name one; "with sam" can make one.
  let makes_context = words[0] == "with" && words.len() > 1;
  let name = if makes_context {
    capitalize(&text)
  } else {
    text.clone()
  };
  let contexts = graph.contexts().map(|c| (c.name.as_str(), c.id));
  for offer in named(
    &name,
    contexts,
    |context| Atom::In { context },
    |name| {
      let context = ContextId::new();
      (context, Event::ContextDefined { context, name })
    },
    "context",
    graph,
    today,
  ) {
    if makes_context || offer.define.is_empty() {
      push(offer);
    }
  }
  out
}

/// Whether `offer` is a clear reading of `text`, clear enough to act on
/// without choosing it from a list: a date, an amount or a duration, a
/// referent named with its lead word ("at home", "during …", "with sam"),
/// or one named exactly. A loose match on a name ("on" for Online) is an
/// offer, not a reading.
pub fn explicit(text: &str, offer: &Offer, graph: &Graph) -> bool {
  if !offer.define.is_empty() {
    return true;
  }
  let text = text.trim().to_lowercase();
  let name = match &offer.atom {
    Atom::After { .. }
    | Atom::Before { .. }
    | Atom::Free { .. }
    | Atom::Has { .. } => return true,
    Atom::At { place } => graph.place(*place).map(|p| p.name.as_str()),
    Atom::Within { schedule } => {
      graph.schedule(*schedule).map(|s| s.name.as_str())
    }
    Atom::In { context } => graph.context(*context).map(|c| c.name.as_str()),
  };
  let led = ["at ", "@", "during ", "on ", "with "]
    .iter()
    .any(|lead| text.starts_with(lead));
  led || name.is_some_and(|n| n.eq_ignore_ascii_case(&text))
}

/// Offers for `name` among `referents`: each that matches, best first, then
/// a new one called `name` unless one is called exactly that.
fn named<Id: Copy>(
  name: &str,
  referents: impl Iterator<Item = (impl AsRef<str>, Id)>,
  atom: impl Fn(Id) -> Atom,
  define: impl FnOnce(String) -> (Id, Event),
  noun: &str,
  graph: &Graph,
  today: Date,
) -> Vec<Offer> {
  let mut scored: Vec<(u32, String, Id)> = referents
    .filter_map(|(n, id)| {
      let n = n.as_ref();
      query::score(name, n).map(|s| (s, n.to_string(), id))
    })
    .collect();
  scored.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.cmp(&b.1)));
  let exact = scored.iter().any(|(_, n, _)| n.eq_ignore_ascii_case(name));
  let mut out: Vec<Offer> = scored
    .into_iter()
    .map(|(_, _, id)| {
      let atom = atom(id);
      Offer {
        label: atom_label(graph, &atom, today),
        atom,
        define: vec![],
      }
    })
    .collect();
  if !exact {
    let name = capitalize(name);
    let (id, event) = define(name.clone());
    let atom = atom(id);
    let mut label = atom_label(&defined(&event), &atom, today);
    label.push_str(&format!(" (new {noun})"));
    out.push(Offer {
      atom,
      define: vec![event],
      label,
    });
  }
  out
}

/// A graph holding just the referent `event` defines, to label an atom
/// that points at it before it exists.
fn defined(event: &Event) -> Graph {
  let mut g = Graph::new();
  event.apply(&mut g);
  g
}

/// "during business hours", "weekends", "on weekdays": schedules by name,
/// and the two everyone means the same thing by.
fn schedules(words: &[&str], graph: &Graph, today: Date) -> Vec<Offer> {
  let rest = match words {
    ["during" | "on" | "in", rest @ ..] if !rest.is_empty() => rest,
    _ => words,
  };
  let name = rest.join(" ");
  let builtin = match name.as_str() {
    "weekend" | "weekends" | "the weekend" => {
      Some(("Weekends", WeekdaySet::WEEKEND))
    }
    "weekday" | "weekdays" | "workdays" | "workday" => {
      Some(("Weekdays", WeekdaySet::WORKDAYS))
    }
    _ => None,
  };
  let schedules = graph.schedules().map(|s| (s.name.as_str(), s.id));
  if let Some((builtin, days)) = builtin {
    let existing = graph
      .schedules()
      .find(|s| s.name.eq_ignore_ascii_case(builtin));
    return vec![match existing {
      Some(s) => {
        let atom = Atom::Within { schedule: s.id };
        Offer {
          label: atom_label(graph, &atom, today),
          atom,
          define: vec![],
        }
      }
      None => {
        let schedule = ScheduleId::new();
        let event = Event::ScheduleDefined {
          schedule,
          name: builtin.into(),
          spans: vec![Span::Weekly {
            days,
            start: TimeOfDay::MIDNIGHT,
            end: TimeOfDay::MIDNIGHT,
          }],
        };
        let atom = Atom::Within { schedule };
        Offer {
          label: format!(
            "{} (new schedule)",
            atom_label(&defined(&event), &atom, today)
          ),
          atom,
          define: vec![event],
        }
      }
    }];
  }
  let offers = named(
    &name,
    schedules,
    |schedule| Atom::Within { schedule },
    |name| {
      let schedule = ScheduleId::new();
      (schedule, Event::ScheduleDefined {
        schedule,
        name,
        spans: vec![],
      })
    },
    "schedule",
    graph,
    today,
  );
  // Only "during …" may make a schedule; otherwise any text would.
  let explicit = words[0] == "during";
  offers
    .into_iter()
    .filter(|o| explicit || o.define.is_empty())
    .collect()
}

/// "$50", "€12.50", "3 batteries": resources with enough of this.
fn resources(words: &[&str], graph: &Graph, today: Date) -> Vec<Offer> {
  let offer = |resource: &Resource, at_least: Amount| {
    let atom = Atom::Has {
      resource: resource.id,
      at_least,
    };
    Offer {
      label: atom_label(graph, &atom, today),
      atom,
      define: vec![],
    }
  };
  let mut out = Vec::new();
  match words {
    [money_word] => {
      let Some((currency, number)) = money_parts(money_word) else {
        return out;
      };
      for r in graph.resources() {
        if let Unit::Money {
          currency: c,
          minor_digits,
        } = &r.unit
          && *c == currency
          && let Some(v) = decimal(number, *minor_digits)
        {
          out.push(offer(r, v));
        }
      }
      if out.is_empty()
        && let Some(v) = decimal(number, 2)
      {
        let resource = ResourceId::new();
        let event = Event::ResourceDefined {
          resource,
          name: "Money".into(),
          unit: Unit::Money {
            currency,
            minor_digits: 2,
          },
          balance: 0,
        };
        let atom = Atom::Has {
          resource,
          at_least: v,
        };
        out.push(Offer {
          label: format!(
            "{} (new resource)",
            atom_label(&defined(&event), &atom, today)
          ),
          atom,
          define: vec![event],
        });
      }
    }
    [count, noun @ ..] if !noun.is_empty() => {
      let Ok(count) = count.parse::<Amount>() else {
        return out;
      };
      let noun = noun.join(" ");
      let singular = singular(&noun);
      let matches = |r: &&Resource| match &r.unit {
        Unit::Count { noun: n } => {
          n.eq_ignore_ascii_case(&noun) || n.eq_ignore_ascii_case(&singular)
        }
        _ => false,
      };
      out.extend(graph.resources().filter(matches).map(|r| offer(r, count)));
      if out.is_empty() {
        let resource = ResourceId::new();
        let event = Event::ResourceDefined {
          resource,
          name: capitalize(&plural(&singular)),
          unit: Unit::Count {
            noun: singular.clone(),
          },
          balance: 0,
        };
        let atom = Atom::Has {
          resource,
          at_least: count,
        };
        out.push(Offer {
          label: format!(
            "{} (new resource)",
            atom_label(&defined(&event), &atom, today)
          ),
          atom,
          define: vec![event],
        });
      }
    }
    _ => {}
  }
  out
}

/// "$50" as its currency code and "50".
fn money_parts(word: &str) -> Option<(String, &str)> {
  ["USD", "EUR", "GBP", "JPY"].into_iter().find_map(|code| {
    let symbol = currency_symbol(code)?;
    word
      .strip_prefix(symbol)
      .map(|number| (code.to_string(), number))
  })
}

/// "12.5" in units with `digits` decimal places (1250 for two), if it has
/// no more places than that.
fn decimal(number: &str, digits: u8) -> Option<Amount> {
  let number = number.replace(',', "");
  let (whole, frac) = number.split_once('.').unwrap_or((&number, ""));
  if frac.len() > digits.into()
    || !whole
      .chars()
      .chain(frac.chars())
      .all(|c| c.is_ascii_digit())
    || whole.is_empty() && frac.is_empty()
  {
    return None;
  }
  let whole: Amount = if whole.is_empty() {
    0
  } else {
    whole.parse().ok()?
  };
  let frac: Amount = format!("{frac:0<width$}", width = digits.into())
    .parse()
    .unwrap_or(0);
  whole
    .checked_mul(10i64.checked_pow(digits.into())?)?
    .checked_add(frac)
}

/// A schedule window typed as days, or a date, then a time range:
/// "weekdays 9:00-17:00", "mon, wed 9am - 5pm", "sat-sun 10:00-16:00".
/// An end at or before the start wraps past midnight ("fri 22:00-02:00").
/// A date instead of days makes a one-off window ("dec 24 9am-12pm").
pub fn span(text: &str, today: Date) -> Option<Span> {
  let text = text
    .trim()
    .to_lowercase()
    .replace(" - ", "-")
    .replace(',', " ");
  let words: Vec<&str> = text.split_whitespace().collect();
  let (range, days) = words.split_last()?;
  let (start, end) = range.split_once('-')?;
  let (start, end) = (time_of_day(start)?, time_of_day(end)?);
  if let Some(days) = weekday_set(days) {
    let (start, end) = (
      TimeOfDay::new(start.hour(), start.minute())?,
      TimeOfDay::new(end.hour(), end.minute())?,
    );
    return Some(Span::Weekly { days, start, end });
  }
  let day = day_of(days, today)?;
  let end_day = if end <= start {
    day.tomorrow().ok()?
  } else {
    day
  };
  Some(Span::Once {
    start: Moment::new(day.to_datetime(start)),
    end:   Moment::new(end_day.to_datetime(end)),
  })
}

/// "weekdays", "weekends", "every day", "daily", "mon wed fri", "mon-fri".
fn weekday_set(words: &[&str]) -> Option<WeekdaySet> {
  match words {
    ["weekdays" | "workdays"] => return Some(WeekdaySet::WORKDAYS),
    ["weekends" | "weekend"] => return Some(WeekdaySet::WEEKEND),
    ["daily"] | ["every", "day"] | ["everyday"] => {
      return Some(WeekdaySet::EVERY_DAY);
    }
    [range] if let Some((a, b)) = range.split_once('-') => {
      let (a, b) = (
        weekday(a)?.to_monday_zero_offset(),
        weekday(b)?.to_monday_zero_offset(),
      );
      // From the first day round to the last: "fri-mon" wraps the week.
      let mut days = vec![a];
      let mut d = a;
      while d != b {
        d = (d + 1) % 7;
        days.push(d);
      }
      return days
        .into_iter()
        .map(|d| Weekday::from_monday_zero_offset(d).ok())
        .collect();
    }
    _ => {}
  }
  if words.is_empty() {
    return None;
  }
  words
    .iter()
    .map(|w| weekday(w))
    .collect::<Option<Vec<_>>>()
    .map(|days| days.into_iter().collect())
}

/// A balance typed for a resource counted in `unit`: "320", "$320.50" or
/// "-12" for money, "2h 30m" or "150" for time, "3" for a count.
pub fn amount(text: &str, unit: &Unit) -> Option<Amount> {
  let text = text.trim().replace(' ', "");
  let (negative, text) = match text.strip_prefix('-') {
    Some(rest) => (true, rest.to_string()),
    None => (false, text),
  };
  let value = match unit {
    Unit::Money {
      currency,
      minor_digits,
    } => {
      let number = currency_symbol(currency)
        .and_then(|sym| text.strip_prefix(sym))
        .or_else(|| text.strip_prefix(currency.as_str()))
        .unwrap_or(&text);
      decimal(number, *minor_digits)?
    }
    Unit::Minutes => match text.parse::<Amount>() {
      Ok(minutes) => minutes,
      Err(_) => duration(&text.to_lowercase())?.into(),
    },
    Unit::Count { .. } => text.parse().ok()?,
  };
  Some(if negative { -value } else { value })
}

/// When free time typed as "15:30", "3pm" or "1h 30m" ends: a time of day
/// is the next one from `now`, a duration runs from `now`.
pub fn until(text: &str, now: Timestamp, zone: &TimeZone) -> Option<Timestamp> {
  let text = text.trim().to_lowercase().replace(' ', "");
  if let Some(time) = time_of_day(&text) {
    let today = zone.to_datetime(now).date();
    let at = super::literal_instant(today.to_datetime(time), zone);
    return Some(if at > now {
      at
    } else {
      super::literal_instant(today.tomorrow().ok()?.to_datetime(time), zone)
    });
  }
  let minutes = duration(&text)?;
  now
    .checked_add(jiff::SignedDuration::from_mins(minutes.into()))
    .ok()
}

/// "1h free", "free for 30 min", "45m": a free-time atom's minutes.
fn free(words: &[&str]) -> Option<Minutes> {
  let rest: &[&str] = match words {
    ["free", "for", rest @ ..] | ["free", rest @ ..] => rest,
    [rest @ .., "free"] => rest,
    rest => rest,
  };
  duration(&rest.join(""))
}

/// "1h30m", "90min", "2hours", "1.5h": whole minutes, spaces removed.
fn duration(s: &str) -> Option<Minutes> {
  let mut total: f64 = 0.0;
  let mut rest = s;
  let mut any = false;
  while !rest.is_empty() {
    let split = rest
      .find(|c: char| !(c.is_ascii_digit() || c == '.'))
      .unwrap_or(rest.len());
    let (number, tail) = rest.split_at(split);
    let number: f64 = number.parse().ok()?;
    let unit_end = tail
      .find(|c: char| c.is_ascii_digit())
      .unwrap_or(tail.len());
    let (unit, tail) = tail.split_at(unit_end);
    let per = match unit {
      "h" | "hr" | "hrs" | "hour" | "hours" => 60.0,
      "m" | "min" | "mins" | "minute" | "minutes" => 1.0,
      _ => return None,
    };
    total += number * per;
    any = true;
    rest = tail;
  }
  (any && total >= 1.0 && total < f64::from(u32::MAX))
    .then(|| total.round() as Minutes)
}

/// A date with an optional time, or a time alone (today): "oct 1", "1 oct
/// 2027", "friday 9am", "tomorrow at 17:30", "2026-10-01", "5pm".
fn moment(words: &[&str], today: Date) -> Option<Moment> {
  let words: Vec<&str> = words.iter().copied().filter(|w| *w != "at").collect();
  // The time, if any, is the last word or two ("9am", "9 am").
  for split in [words.len().saturating_sub(2), words.len().saturating_sub(1)] {
    let (day, time) = words.split_at(split);
    if time.is_empty() {
      continue;
    }
    if let Some(time) = time_of_day(&time.join("")) {
      let day = if day.is_empty() {
        Some(today)
      } else {
        day_of(day, today)
      };
      return day.map(|d| Moment::new(d.to_datetime(time)));
    }
  }
  day_of(&words, today).map(Moment::on)
}

/// "9am", "9:30pm", "17:00", "noon", "midnight".
fn time_of_day(s: &str) -> Option<Time> {
  match s {
    "noon" => return Time::new(12, 0, 0, 0).ok(),
    "midnight" => return Some(Time::midnight()),
    _ => {}
  }
  let (clock, offset) = if let Some(c) = s.strip_suffix("am") {
    (c, Some(0))
  } else if let Some(c) = s.strip_suffix("pm") {
    (c, Some(12))
  } else {
    (s, None)
  };
  let (h, m) = clock.split_once(':').unwrap_or((clock, "0"));
  let (mut h, m): (i8, i8) = (h.parse().ok()?, m.parse().ok()?);
  match offset {
    Some(offset) if (1..=12).contains(&h) => h = h % 12 + offset,
    Some(_) => return None,
    // A bare number is a count, not a time.
    None if !clock.contains(':') => return None,
    None => {}
  }
  Time::new(h, m, 0, 0).ok()
}

/// "today", "friday", "oct 1", "1 october 2027", "2026-10-01".
fn day_of(words: &[&str], today: Date) -> Option<Date> {
  match words {
    ["today"] => Some(today),
    ["tomorrow"] => today.tomorrow().ok(),
    [iso] if iso.contains('-') => iso.parse().ok(),
    [day] => {
      let day = weekday(day)?;
      let ahead = (day.to_monday_zero_offset()
        - today.weekday().to_monday_zero_offset())
      .rem_euclid(7);
      today.checked_add(jiff::Span::new().days(ahead)).ok()
    }
    [a, b] | [a, b, _] => {
      let year = match words {
        [_, _, y] => Some(y.parse::<i16>().ok()?),
        _ => None,
      };
      let (month, day) = match (month(a), month(b)) {
        (Some(m), None) => (m, ordinal(b)?),
        (None, Some(m)) => (m, ordinal(a)?),
        _ => return None,
      };
      match year {
        Some(y) => Date::new(y, month, day).ok(),
        // Without a year, the next such day from today on.
        None => {
          let this = Date::new(today.year(), month, day).ok()?;
          if this >= today {
            Some(this)
          } else {
            Date::new(today.year() + 1, month, day).ok()
          }
        }
      }
    }
    _ => None,
  }
}

/// "1", "1st", "22nd".
fn ordinal(s: &str) -> Option<i8> {
  s.trim_end_matches(|c: char| c.is_ascii_alphabetic())
    .parse()
    .ok()
}

fn month(s: &str) -> Option<i8> {
  const NAMES: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct",
    "nov", "dec",
  ];
  if s.len() < 3 {
    return None;
  }
  NAMES
    .iter()
    .position(|m| s.starts_with(m) && full_month(m).starts_with(s))
    .map(|i| i as i8 + 1)
}

/// The whole name of a month from its first three letters.
fn full_month(abbr: &str) -> &'static str {
  match abbr {
    "jan" => "january",
    "feb" => "february",
    "mar" => "march",
    "apr" => "april",
    "may" => "may",
    "jun" => "june",
    "jul" => "july",
    "aug" => "august",
    "sep" => "september",
    "oct" => "october",
    "nov" => "november",
    _ => "december",
  }
}

fn weekday(s: &str) -> Option<Weekday> {
  let days = [
    ("monday", Weekday::Monday),
    ("tuesday", Weekday::Tuesday),
    ("wednesday", Weekday::Wednesday),
    ("thursday", Weekday::Thursday),
    ("friday", Weekday::Friday),
    ("saturday", Weekday::Saturday),
    ("sunday", Weekday::Sunday),
  ];
  (s.len() >= 3)
    .then(|| days.into_iter().find(|(name, _)| name.starts_with(s)))
    .flatten()
    .map(|(_, day)| day)
}

/// "batteries" to "battery", "boxes" to "box", "screws" to "screw".
fn singular(noun: &str) -> String {
  if let Some(stem) = noun.strip_suffix("ies") {
    format!("{stem}y")
  } else if let Some(stem) = noun.strip_suffix("es")
    && (stem.ends_with(['s', 'x', 'z'])
      || stem.ends_with("ch")
      || stem.ends_with("sh"))
  {
    stem.to_string()
  } else if let Some(stem) = noun.strip_suffix('s')
    && !stem.ends_with('s')
  {
    stem.to_string()
  } else {
    noun.to_string()
  }
}

fn capitalize(s: &str) -> String {
  let mut chars = s.chars();
  chars
    .next()
    .map(|c| c.to_uppercase().chain(chars).collect())
    .unwrap_or_default()
}

#[cfg(test)]
mod tests;
