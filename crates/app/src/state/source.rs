//! What satisfies a condition: switching the selected condition between
//! manual and each kind of formula, from its "Satisfied by" choice in the
//! inspector, or by typing a formula as its name.
//!
//! A formula condition's id is its atom's, so switching makes a new node
//! and retires the old one. What required the old condition requires the
//! new one, and quests that claimed it claim the new one, in one undo step.
//! Switching to an atom some condition already holds joins that node,
//! which is how two conditions that turn out to mean "At Home" become one.

use std::collections::HashSet;

use base::{
  Atom, ContextId, EdgeId, EdgeKind, Event, Graph, NodeId, NodeKind, PlaceId,
  ResourceId, ScheduleId, Unit,
};

use super::AppState;
use crate::{
  focus::FieldKey,
  formula::{
    describe,
    phrase::{self, Offer},
  },
};

/// What can satisfy a condition, as the inspector offers the choice.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum SourceKind {
  /// Marked satisfied by hand.
  #[default]
  Manual,
  /// After or before a date.
  Date,
  /// Being at a place.
  Place,
  /// Having enough of a resource.
  Money,
  /// A schedule's windows.
  Schedule,
  /// Having enough free time.
  FreeTime,
  /// A context being on.
  Context,
}

impl SourceKind {
  /// Every kind, in the order the choice lists them.
  pub const ALL: [SourceKind; 7] = [
    SourceKind::Manual,
    SourceKind::Date,
    SourceKind::Place,
    SourceKind::Money,
    SourceKind::Schedule,
    SourceKind::FreeTime,
    SourceKind::Context,
  ];

  /// The choice's label.
  pub fn label(self) -> &'static str {
    match self {
      SourceKind::Manual => "Manual",
      SourceKind::Date => "Date",
      SourceKind::Place => "Place",
      SourceKind::Money => "Money",
      SourceKind::Schedule => "Schedule",
      SourceKind::FreeTime => "Free time",
      SourceKind::Context => "Context",
    }
  }

  /// The kind of a condition holding `atom` (`None`: a manual one).
  pub fn of(atom: Option<&Atom>) -> Self {
    match atom {
      None => SourceKind::Manual,
      Some(Atom::After { .. } | Atom::Before { .. }) => SourceKind::Date,
      Some(Atom::At { .. }) => SourceKind::Place,
      Some(Atom::Has { .. }) => SourceKind::Money,
      Some(Atom::Within { .. }) => SourceKind::Schedule,
      Some(Atom::Free { .. }) => SourceKind::FreeTime,
      Some(Atom::In { .. }) => SourceKind::Context,
    }
  }

  /// Whether this kind chooses among referents (places, resources…).
  pub fn picks(self) -> bool {
    matches!(
      self,
      SourceKind::Place
        | SourceKind::Money
        | SourceKind::Schedule
        | SourceKind::Context
    )
  }
}

/// The "Satisfied by" form, as filled in so far.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SourceDraft {
  /// The kind chosen.
  pub kind:   SourceKind,
  /// For a date: before it rather than after it.
  pub before: bool,
  /// What is typed: the date, the amount, the free time, a new referent's
  /// name, or a manual condition's name.
  pub text:   String,
  /// The existing referent chosen, by raw id.
  pub pick:   Option<u128>,
}

/// What applying the form turns the condition into.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
  /// A formula condition, with any referent it needs defined.
  Formula(Offer),
  /// A manual condition with this name.
  Manual(String),
}

impl Target {
  /// What the target is, in words: "At Home", "Manual: Signed off".
  pub fn label(&self) -> String {
    match self {
      Target::Formula(offer) => offer.label.clone(),
      Target::Manual(name) => format!("Manual: {name}"),
    }
  }
}

impl AppState {
  /// What satisfies the selected node, if it is a condition.
  pub fn source_kind(&self) -> Option<SourceKind> {
    let id = self.selected?;
    let store = self.lock();
    let node = store.graph().node(id)?;
    matches!(node.kind, NodeKind::Condition { .. })
      .then(|| SourceKind::of(node.kind.atom()))
  }

  /// The open "Satisfied by" form, if any.
  pub fn source_draft(&self) -> Option<&SourceDraft> { self.source.as_ref() }

  /// Open the form for `kind`, filled in from the condition when it is
  /// already of that kind, so applying it unchanged changes nothing.
  pub fn choose_source(&mut self, kind: SourceKind) {
    let Some(id) = self.selected else { return };
    let draft = {
      let store = self.lock();
      let graph = store.graph();
      let Some(node) = graph.node(id) else { return };
      let today = self.today();
      let mut draft = SourceDraft {
        kind,
        ..SourceDraft::default()
      };
      match (kind, node.kind.atom()) {
        (SourceKind::Manual, atom) => {
          draft.text = match atom {
            Some(atom) => describe::atom_label(graph, atom, today),
            None => node.name.clone(),
          };
        }
        (SourceKind::Date, Some(Atom::After { at } | Atom::Before { at })) => {
          draft.before = matches!(node.kind.atom(), Some(Atom::Before { .. }));
          let dt = at.civil();
          draft.text = if dt.time() == jiff::civil::Time::midnight() {
            dt.date().to_string()
          } else {
            format!("{} {:02}:{:02}", dt.date(), dt.hour(), dt.minute())
          };
        }
        (SourceKind::Place, Some(Atom::At { place })) => {
          draft.pick = Some(place.to_u128());
        }
        (SourceKind::Money, Some(Atom::Has { resource, at_least })) => {
          draft.pick = Some(resource.to_u128());
          if let Some(r) = graph.resource(*resource) {
            draft.text = describe::plain_amount(&base::Resource {
              balance: *at_least,
              ..r.clone()
            });
          }
        }
        (SourceKind::Schedule, Some(Atom::Within { schedule })) => {
          draft.pick = Some(schedule.to_u128());
        }
        (SourceKind::FreeTime, Some(Atom::Free { at_least })) => {
          draft.text = describe::minutes(*at_least);
        }
        (SourceKind::Context, Some(Atom::In { context })) => {
          draft.pick = Some(context.to_u128());
        }
        _ => {}
      }
      draft
    };
    self.source = Some(draft);
    self.focus_requests.request(FieldKey::Source);
  }

  /// Close the form without changing anything.
  pub fn cancel_source(&mut self) { self.source = None; }

  /// The form's text field changed. Typing a new name for a referent
  /// un-chooses the one picked; an amount keeps it.
  pub fn set_source_text(&mut self, text: String) {
    if let Some(draft) = &mut self.source {
      if draft.kind.picks() && draft.kind != SourceKind::Money {
        draft.pick = None;
      }
      draft.text = text;
    }
  }

  /// Choose between after and before, for a date.
  pub fn set_source_before(&mut self, before: bool) {
    if let Some(draft) = &mut self.source {
      draft.before = before;
    }
  }

  /// Choose an existing referent (by raw id); a new name typed for one is
  /// dropped, but an amount is kept.
  pub fn pick_source(&mut self, pick: u128) {
    if let Some(draft) = &mut self.source {
      draft.pick = Some(pick);
      if draft.kind != SourceKind::Money {
        draft.text.clear();
      }
    }
  }

  /// The referents the open form chooses among, by name, with raw ids.
  pub fn source_choices(&self) -> Vec<(u128, String)> {
    let Some(kind) = self.source.as_ref().map(|d| d.kind) else {
      return Vec::new();
    };
    let store = self.lock();
    let g = store.graph();
    let mut choices: Vec<(u128, String)> = match kind {
      SourceKind::Place => g
        .places()
        .map(|p| (p.id.to_u128(), p.name.clone()))
        .collect(),
      SourceKind::Money => g
        .resources()
        .map(|r| (r.id.to_u128(), r.name.clone()))
        .collect(),
      SourceKind::Schedule => g
        .schedules()
        .map(|s| (s.id.to_u128(), s.name.clone()))
        .collect(),
      SourceKind::Context => g
        .contexts()
        .map(|c| (c.id.to_u128(), c.name.clone()))
        .collect(),
      _ => Vec::new(),
    };
    choices.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    choices
  }

  /// What applying the form would turn the condition into, or what it
  /// still needs.
  pub fn source_target(&self) -> Result<Target, &'static str> {
    let draft = self.source.as_ref().ok_or("Choose what satisfies it.")?;
    let store = self.lock();
    let graph = store.graph();
    let today = self.today();
    let text = draft.text.trim();
    let offer = |atom: Atom, define: Vec<Event>| {
      let label = if define.is_empty() {
        describe::atom_label(graph, &atom, today)
      } else {
        // Label it as it will be, with its new referent defined.
        let mut g = Graph::new();
        for event in &define {
          event.apply(&mut g);
        }
        format!("{} (new)", describe::atom_label(&g, &atom, today))
      };
      Target::Formula(Offer {
        atom,
        define,
        label,
      })
    };
    // Every offer the phrase parser reads `phrase` as, of one atom kind.
    let parsed = |phrase: &str, wanted: fn(&Atom) -> bool| {
      phrase::offers(phrase, graph, today)
        .into_iter()
        .find(|o| wanted(&o.atom))
        .map(Target::Formula)
    };
    match draft.kind {
      SourceKind::Manual if text.is_empty() => Err("Give it a name."),
      SourceKind::Manual => Ok(Target::Manual(text.to_string())),
      SourceKind::Date => {
        let lead = if draft.before { "before" } else { "after" };
        parsed(&format!("{lead} {text}"), |a| {
          matches!(a, Atom::After { .. } | Atom::Before { .. })
        })
        .ok_or("Type a date, like oct 1, friday 9am or 2026-10-01.")
      }
      SourceKind::FreeTime => {
        parsed(&format!("{text} free"), |a| matches!(a, Atom::Free { .. }))
          .ok_or("Type how long, like 30m or 1h 30m.")
      }
      SourceKind::Money => match draft.pick {
        Some(raw) => {
          let resource = ResourceId::from_u128(raw);
          let r = graph.resource(resource).ok_or("Pick a resource.")?;
          let at_least = phrase::amount(text, &r.unit).ok_or(match r.unit {
            Unit::Money { .. } => "Type an amount, like 50 or 12.50.",
            Unit::Minutes => "Type an amount of time, like 2h.",
            Unit::Count { .. } => "Type how many, like 3.",
          })?;
          Ok(offer(Atom::Has { resource, at_least }, vec![]))
        }
        None => parsed(text, |a| matches!(a, Atom::Has { .. }))
          .ok_or("Pick a resource, or type an amount like $50 or 3 batteries."),
      },
      SourceKind::Place => {
        let atom = |place| Atom::At { place };
        let found = named_referent(
          draft.pick,
          text,
          graph.places().map(|p| (p.id.to_u128(), p.name.as_str())),
        );
        match found {
          Found::Existing(raw) => {
            Ok(offer(atom(PlaceId::from_u128(raw)), vec![]))
          }
          Found::New(name) => {
            let place = PlaceId::new();
            Ok(offer(atom(place), vec![Event::PlaceDefined {
              place,
              name,
              within: None,
            }]))
          }
          Found::Nothing => Err("Pick a place, or name a new one."),
        }
      }
      SourceKind::Schedule => {
        let atom = |schedule| Atom::Within { schedule };
        let found = named_referent(
          draft.pick,
          text,
          graph.schedules().map(|s| (s.id.to_u128(), s.name.as_str())),
        );
        match found {
          Found::Existing(raw) => {
            Ok(offer(atom(ScheduleId::from_u128(raw)), vec![]))
          }
          Found::New(name) => {
            let schedule = ScheduleId::new();
            Ok(offer(atom(schedule), vec![Event::ScheduleDefined {
              schedule,
              name,
              spans: vec![],
            }]))
          }
          Found::Nothing => Err("Pick a schedule, or name a new one."),
        }
      }
      SourceKind::Context => {
        let atom = |context| Atom::In { context };
        let found = named_referent(
          draft.pick,
          text,
          graph.contexts().map(|c| (c.id.to_u128(), c.name.as_str())),
        );
        match found {
          Found::Existing(raw) => {
            Ok(offer(atom(ContextId::from_u128(raw)), vec![]))
          }
          Found::New(name) => {
            let context = ContextId::new();
            Ok(offer(atom(context), vec![Event::ContextDefined {
              context,
              name,
            }]))
          }
          Found::Nothing => Err("Pick a context, or name a new one."),
        }
      }
    }
  }

  /// How many requirements of its own the selected condition would lose by
  /// becoming a formula condition, which can have none.
  pub fn source_drops(&self) -> usize {
    let formula = self
      .source
      .as_ref()
      .is_some_and(|d| d.kind != SourceKind::Manual);
    let Some(id) = self.selected.filter(|_| formula) else {
      return 0;
    };
    self.lock().graph().requirements_of(id).count()
  }

  /// Apply the form: turn the selected condition into what it describes.
  /// Nothing happens while the form is incomplete.
  pub fn apply_source(&mut self) {
    let Ok(target) = self.source_target() else {
      return;
    };
    self.source = None;
    if let Some(id) = self.selected {
      self.replace_condition(id, target);
    }
  }

  /// Formulas the selected condition's name, as typed, reads as.
  pub fn title_offers(&self) -> Vec<Offer> {
    if self.source_kind() != Some(SourceKind::Manual) {
      return Vec::new();
    }
    let store = self.lock();
    phrase::offers(&self.name_draft, store.graph(), self.today())
  }

  /// The formula Enter in the name field would turn the selected condition
  /// into: the first offer, when the name clearly reads as it.
  pub fn title_reading(&self) -> Option<Offer> {
    let offer = self.title_offers().into_iter().next()?;
    let store = self.lock();
    phrase::explicit(&self.name_draft, &offer, store.graph()).then_some(offer)
  }

  /// Turn the selected condition into `offer`'s formula.
  pub fn make_automatic(&mut self, offer: Offer) {
    if let Some(id) = self.selected {
      self.replace_condition(id, Target::Formula(offer));
    }
  }

  /// Replace condition `old` with `target`, moving to it every edge into
  /// `old` and every quest claim on it, as one undo step; then select it.
  /// `old`'s own requirements go with it.
  fn replace_condition(&mut self, old: NodeId, target: Target) {
    let (new, events, label) = {
      let store = self.lock();
      let graph = store.graph();
      let Some(node) = graph.node(old) else { return };
      let (new, mut events, label) = match target {
        Target::Formula(offer) => {
          let new = offer.atom.node_id();
          if new == old {
            return;
          }
          let mut events = Vec::new();
          if graph.node(new).is_none() {
            events.push(offer.atom.node_added());
          }
          events.extend(offer.define);
          (new, events, "make automatic")
        }
        Target::Manual(name) => {
          if node.kind.atom().is_none() {
            // Already manual: only the name can change.
            let events = (node.name != name)
              .then_some(Event::NodeRenamed { node: old, name })
              .into_iter()
              .collect();
            (old, events, "rename")
          } else {
            let new = NodeId::new();
            let events = vec![Event::NodeAdded {
              node: new,
              kind: NodeKind::condition(),
              name,
              order_hint: node.order_hint,
            }];
            (new, events, "make manual")
          }
        }
      };
      if new != old {
        events.extend(moved_links(graph, old, new));
        events.push(Event::NodeRemoved { node: old });
      }
      (new, events, label)
    };
    self.commit_as(events, label);
    self.select(Some(new));
  }
}

/// The edges and claims that make `new` stand where `old` did: an edge
/// from each of `old`'s dependents that does not already require `new`,
/// and a claim for each quest that claims `old` but not `new`.
fn moved_links(graph: &Graph, old: NodeId, new: NodeId) -> Vec<Event> {
  let mut events = Vec::new();
  let mut requiring: HashSet<NodeId> =
    graph.dependents_of(new).map(|e| e.from).collect();
  for edge in graph.dependents_of(old) {
    if edge.from != new && requiring.insert(edge.from) {
      events.push(Event::EdgeAdded {
        edge: EdgeId::new(),
        kind: EdgeKind::Dependency,
        from: edge.from,
        to:   new,
      });
    }
  }
  for quest in base::claiming_quests(graph, old) {
    if !base::claiming_quests(graph, new).contains(&quest) {
      events.push(Event::QuestClaimed { quest, node: new });
    }
  }
  events
}

/// What a referent field amounts to: the one picked, the one whose name
/// was typed exactly, a new one by the typed name, or nothing yet.
enum Found {
  Existing(u128),
  New(String),
  Nothing,
}

fn named_referent<'a>(
  pick: Option<u128>,
  text: &str,
  referents: impl Iterator<Item = (u128, &'a str)>,
) -> Found {
  let referents: Vec<(u128, &str)> = referents.collect();
  if let Some(raw) = pick.filter(|p| referents.iter().any(|(id, _)| id == p)) {
    return Found::Existing(raw);
  }
  if text.is_empty() {
    return Found::Nothing;
  }
  match referents.iter().find(|(_, n)| n.eq_ignore_ascii_case(text)) {
    Some((raw, _)) => Found::Existing(*raw),
    None => {
      let mut chars = text.chars();
      let name = chars
        .next()
        .map(|c| c.to_uppercase().chain(chars).collect())
        .unwrap_or_default();
      Found::New(name)
    }
  }
}
