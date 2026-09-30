//! Formula conditions in words: an atom's label, and a sentence for why it
//! holds or not.
//!
//! A formula node has no name of its own; it is labelled from its atom and
//! the referents the atom points at, so renaming Home relabels every "At
//! Home" (plans/formula-conditions.md, invariant 4).

use base::{
  Amount, Atom, Explanation, Graph, Minutes, Moment, Node, Resource, Span,
  Truth, Unit, WeekdaySet,
};
use jiff::{
  SignedDuration, Timestamp,
  civil::{Date, Weekday},
  tz::TimeZone,
};

/// What a node is called: its name, or for a formula condition, its atom's
/// label. `today` decides whether dates need their year.
pub fn node_name(graph: &Graph, node: &Node, today: Date) -> String {
  match node.kind.atom() {
    Some(atom) => atom_label(graph, atom, today),
    None => node.name.clone(),
  }
}

/// An atom as a short phrase: "After Thu 1 Oct", "At Home", "Has $50.00".
pub fn atom_label(graph: &Graph, atom: &Atom, today: Date) -> String {
  match atom {
    Atom::After { at } => format!("After {}", moment(*at, today)),
    Atom::Before { at } => format!("Before {}", moment(*at, today)),
    Atom::Within { schedule } => match graph.schedule(*schedule) {
      Some(s) => format!("During {}", s.name),
      None => "During an unknown schedule".into(),
    },
    Atom::Free { at_least } => format!("{} free", minutes(*at_least)),
    Atom::At { place } => match graph.place(*place) {
      Some(p) => format!("At {}", p.name),
      None => "At an unknown place".into(),
    },
    Atom::Has { resource, at_least } => match graph.resource(*resource) {
      Some(r) => format!("Has {}", holding(r, *at_least)),
      None => "Has an unknown resource".into(),
    },
    Atom::In { context } => match graph.context(*context) {
      Some(c) => c.name.clone(),
      None => "An unknown context".into(),
    },
  }
}

/// One line on why `atom` does or does not hold, given its `truth`: "Opens
/// Thu 1 Oct, in 3 days", "Needs $50.00, have $32.10".
pub fn truth_sentence(
  graph: &Graph,
  atom: &Atom,
  truth: &Truth,
  now: Timestamp,
  zone: &TimeZone,
) -> String {
  let today = zone.to_datetime(now).date();
  let when = |at: Timestamp| {
    let civil = Moment::new(zone.to_datetime(at));
    format!("{}, {}", moment(civil, today), relative(now, at))
  };
  match (&truth.why, truth.holds, truth.until) {
    (Explanation::Missing, ..) => match atom {
      Atom::Within { .. } => "Its schedule was deleted.".into(),
      Atom::At { .. } => "Its place was deleted.".into(),
      Atom::Has { .. } => "Its resource was deleted.".into(),
      _ => "Its context was deleted.".into(),
    },
    (Explanation::Clock, true, Some(at)) => format!("Closes {}.", when(at)),
    (Explanation::Clock, true, None) => "Open.".into(),
    (Explanation::Clock, false, Some(at)) => format!("Opens {}.", when(at)),
    (Explanation::Clock, false, None) => "Closed for good.".into(),
    (Explanation::Free { left: None }, ..) => {
      "Free time unknown: set it in the Now tray.".into()
    }
    (Explanation::Free { left: Some(left) }, ..) => {
      format!("{} free left.", minutes(*left))
    }
    (Explanation::Place { here }, ..) => {
      let names: Vec<&str> = here
        .iter()
        .filter_map(|p| graph.place(*p).map(|p| p.name.as_str()))
        .collect();
      if names.is_empty() {
        "No place set: choose one in the Now tray.".into()
      } else {
        format!("You're at {}.", names.join(", "))
      }
    }
    (Explanation::Balance { have }, holds, _) => {
      let Atom::Has { resource, at_least } = atom else {
        return String::new();
      };
      let Some(r) = graph.resource(*resource) else {
        return String::new();
      };
      if holds {
        format!("Have {}.", amount(r, *have))
      } else {
        format!("Needs {}, have {}.", amount(r, *at_least), amount(r, *have))
      }
    }
    (Explanation::Context, true, _) => "On.".into(),
    (Explanation::Context, false, _) => {
      "Off: turn it on in the Now tray.".into()
    }
  }
}

/// How long until `at`, from `now`: "in 5 min", "in 2 h", "in 3 days".
pub fn relative(now: Timestamp, at: Timestamp) -> String {
  let mins = at.duration_since(now).as_mins();
  match mins {
    ..=0 => "now".into(),
    1..60 => format!("in {mins} min"),
    60..2880 => format!("in {} h", mins / 60),
    _ => format!("in {} days", mins / (60 * 24)),
  }
}

/// A moment as "Thu 1 Oct", with the time unless it is midnight, and the
/// year unless it is `today`'s.
pub fn moment(at: Moment, today: Date) -> String {
  let dt = at.civil();
  let mut format = String::from("%a %-d %b");
  if dt.year() != today.year() {
    format.push_str(" %Y");
  }
  if dt.time() != jiff::civil::Time::midnight() {
    format.push_str(" %H:%M");
  }
  dt.strftime(&format).to_string()
}

/// A span of minutes: "45 min", "2 h", "1 h 30 min".
pub fn minutes(m: Minutes) -> String {
  match (m / 60, m % 60) {
    (0, m) => format!("{m} min"),
    (h, 0) => format!("{h} h"),
    (h, m) => format!("{h} h {m} min"),
  }
}

/// A quantity of `resource`, in its unit: "$50.00", "3 batteries", "2 h".
pub fn amount(resource: &Resource, value: Amount) -> String {
  match &resource.unit {
    Unit::Money {
      currency,
      minor_digits,
    } => money(currency, *minor_digits, value),
    Unit::Minutes => minutes(value.clamp(0, u32::MAX.into()) as Minutes),
    Unit::Count { noun } => {
      let noun = if value == 1 {
        noun.clone()
      } else {
        plural(noun)
      };
      format!("{value} {noun}")
    }
  }
}

/// `resource`'s balance as it would be typed back: "320.00", "2 h", "3".
pub fn plain_amount(resource: &Resource) -> String {
  let value = resource.balance;
  match &resource.unit {
    Unit::Money { minor_digits, .. } => {
      let sign = if value < 0 { "-" } else { "" };
      format!("{sign}{}", decimal(value.unsigned_abs(), *minor_digits))
    }
    Unit::Minutes => minutes(value.clamp(0, u32::MAX.into()) as Minutes),
    Unit::Count { .. } => value.to_string(),
  }
}

/// What holding `value` of `resource` reads as: money and time name the
/// resource, since there may be several ("$50.00 in Fun budget"); a count
/// names itself ("3 batteries").
fn holding(resource: &Resource, value: Amount) -> String {
  match resource.unit {
    Unit::Count { .. } => amount(resource, value),
    _ => format!("{} in {}", amount(resource, value), resource.name),
  }
}

/// `value` minor units of `currency`: "$50.00", "CHF 12.50", "¥500".
pub fn money(currency: &str, minor_digits: u8, value: Amount) -> String {
  let sign = if value < 0 { "-" } else { "" };
  let number = decimal(value.unsigned_abs(), minor_digits);
  match currency_symbol(currency) {
    Some(symbol) => format!("{sign}{symbol}{number}"),
    None => format!("{sign}{currency} {number}"),
  }
}

/// `value` minor units with `digits` decimal places: "320.00".
fn decimal(value: u64, digits: u8) -> String {
  let scale = 10u64.pow(digits.into());
  if digits == 0 {
    return value.to_string();
  }
  format!(
    "{}.{:0width$}",
    value / scale,
    value % scale,
    width = digits.into()
  )
}

/// The symbol written before amounts of `currency`, where it is
/// unambiguous enough to use.
pub fn currency_symbol(currency: &str) -> Option<&'static str> {
  Some(match currency {
    "USD" => "$",
    "EUR" => "€",
    "GBP" => "£",
    "JPY" => "¥",
    _ => return None,
  })
}

/// An English plural, good enough for the nouns people count.
pub fn plural(noun: &str) -> String {
  if noun.ends_with(['s', 'x', 'z'])
    || noun.ends_with("ch")
    || noun.ends_with("sh")
  {
    format!("{noun}es")
  } else if let Some(stem) = noun.strip_suffix('y')
    && !stem.ends_with(['a', 'e', 'i', 'o', 'u'])
  {
    format!("{stem}ies")
  } else {
    format!("{noun}s")
  }
}

/// A schedule window: "Weekdays 09:00 – 17:00", "Fri 22:00 – 02:00",
/// "Thu 24 Dec 09:00 – Thu 24 Dec 12:00".
pub fn span(span: &Span, today: Date) -> String {
  match span {
    Span::Weekly { days, start, end } => {
      if start == end {
        format!("{} all day", weekdays(*days))
      } else {
        format!("{} {start} \u{2013} {end}", weekdays(*days))
      }
    }
    Span::Once { start, end } => {
      let same_day = start.civil().date() == end.civil().date();
      let end_text = if same_day {
        end.civil().strftime("%H:%M").to_string()
      } else {
        moment(*end, today)
      };
      let start_text = {
        let t = start.civil();
        let day = moment(Moment::on(t.date()), today);
        format!("{day} {}", t.strftime("%H:%M"))
      };
      format!("{start_text} \u{2013} {end_text}")
    }
  }
}

/// A set of weekdays: "Every day", "Weekdays", "Weekends", "Mon, Wed, Fri".
pub fn weekdays(days: WeekdaySet) -> String {
  if days == WeekdaySet::EVERY_DAY {
    return "Every day".into();
  }
  if days == WeekdaySet::WORKDAYS {
    return "Weekdays".into();
  }
  if days == WeekdaySet::WEEKEND {
    return "Weekends".into();
  }
  let names: Vec<&str> = (0..7)
    .filter_map(|i| Weekday::from_monday_zero_offset(i).ok())
    .filter(|d| days.contains(*d))
    .map(weekday)
    .collect();
  if names.is_empty() {
    "No days".into()
  } else {
    names.join(", ")
  }
}

fn weekday(day: Weekday) -> &'static str {
  match day {
    Weekday::Monday => "Mon",
    Weekday::Tuesday => "Tue",
    Weekday::Wednesday => "Wed",
    Weekday::Thursday => "Thu",
    Weekday::Friday => "Fri",
    Weekday::Saturday => "Sat",
    Weekday::Sunday => "Sun",
  }
}

/// A duration for a timer: `at - now`, never negative.
pub fn until(now: Timestamp, at: Timestamp) -> std::time::Duration {
  at.duration_since(now)
    .max(SignedDuration::ZERO)
    .try_into()
    .unwrap_or_default()
}

#[cfg(test)]
mod tests {
  use jiff::civil::{date, datetime};

  use super::*;

  #[test]
  fn moments_drop_what_goes_without_saying() {
    let today = date(2026, 9, 28);
    let m = |y, mo, d, h, mi| Moment::new(datetime(y, mo, d, h, mi, 0, 0));
    assert_eq!(moment(m(2026, 10, 1, 0, 0), today), "Thu 1 Oct");
    assert_eq!(moment(m(2026, 10, 15, 17, 30), today), "Thu 15 Oct 17:30");
    assert_eq!(moment(m(2027, 1, 4, 0, 0), today), "Mon 4 Jan 2027");
  }

  #[test]
  fn quantities() {
    assert_eq!(minutes(45), "45 min");
    assert_eq!(minutes(120), "2 h");
    assert_eq!(minutes(90), "1 h 30 min");
    assert_eq!(money("USD", 2, 5_000), "$50.00");
    assert_eq!(money("USD", 2, -3_210), "-$32.10");
    assert_eq!(money("CHF", 2, 1_205), "CHF 12.05");
    assert_eq!(money("JPY", 0, 500), "¥500");
    assert_eq!(plural("battery"), "batteries");
    assert_eq!(plural("box"), "boxes");
    assert_eq!(plural("day"), "days");
  }

  #[test]
  fn relative_times() {
    let now: Timestamp = "2026-09-28T12:00Z".parse().unwrap();
    let later = |mins| now + SignedDuration::from_mins(mins);
    assert_eq!(relative(now, now), "now");
    assert_eq!(relative(now, later(5)), "in 5 min");
    assert_eq!(relative(now, later(150)), "in 2 h");
    assert_eq!(relative(now, later(60 * 24 * 3)), "in 3 days");
  }
}
