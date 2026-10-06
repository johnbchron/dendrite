//! Formula conditions: conditions whose truth is computed from facts about
//! the world rather than set by a click (plans/formula-conditions.md).
//!
//! A formula is one flat [`Atom`]. Its node id is a hash of its canonical
//! value, so every task that needs "at Home" links to the same node, and
//! conjunction stays in the graph, where a node needs all its requirements.

mod facts;
mod literal;
mod schedule;

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

pub use self::{
  facts::Facts,
  literal::{Amount, LiteralError, Minutes, Moment, TimeOfDay, WeekdaySet},
};
use crate::{
  event::Event,
  graph::Graph,
  ids::{ContextId, EdgeId, NodeId, PlaceId, ResourceId, ScheduleId},
  model::{EdgeKind, NodeKind},
};

/// One predicate over the facts. Deliberately flat: conjunction is
/// expressed by requiring several atoms, as every other requirement is.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(tag = "atom", rename_all = "snake_case")]
pub enum Atom {
  /// Holds from `at` on: a start date.
  After {
    /// The moment it opens.
    at: Moment,
  },
  /// Holds until `at`: a gate that closes for good. Not a due date.
  Before {
    /// The moment it closes.
    at: Moment,
  },
  /// Holds during any window of the schedule.
  Within {
    /// The schedule whose windows open it.
    schedule: ScheduleId,
  },
  /// Holds while more than `at_least` minutes of declared free time are
  /// left.
  Free {
    /// Minutes of free time needed.
    at_least: Minutes,
  },
  /// Holds while I am at the place, or at any place within it.
  At {
    /// Where I need to be.
    place: PlaceId,
  },
  /// Holds while the resource's balance is at least `at_least`.
  Has {
    /// What I need some of.
    resource: ResourceId,
    /// How much, in the resource's smallest unit.
    at_least: Amount,
  },
  /// Holds while the context is active.
  In {
    /// The context that must be active.
    context: ContextId,
  },
}

/// The version of [`Atom::canonical_bytes`]. Frozen: changing the encoding
/// would silently give every existing formula node a new id.
const CANONICAL_VERSION: u8 = 1;

impl Atom {
  /// The frozen, fixed-width encoding the node id is hashed from: the
  /// encoding version, a tag byte per variant, then the fields big-endian.
  pub fn canonical_bytes(&self) -> Vec<u8> {
    let mut out = vec![CANONICAL_VERSION];
    match self {
      Atom::After { at } => {
        out.push(1);
        out.extend(at.canonical_bytes());
      }
      Atom::Before { at } => {
        out.push(2);
        out.extend(at.canonical_bytes());
      }
      Atom::Within { schedule } => {
        out.push(3);
        out.extend(schedule.to_u128().to_be_bytes());
      }
      Atom::Free { at_least } => {
        out.push(4);
        out.extend(at_least.to_be_bytes());
      }
      Atom::At { place } => {
        out.push(5);
        out.extend(place.to_u128().to_be_bytes());
      }
      Atom::Has { resource, at_least } => {
        out.push(6);
        out.extend(resource.to_u128().to_be_bytes());
        out.extend(at_least.to_be_bytes());
      }
      Atom::In { context } => {
        out.push(7);
        out.extend(context.to_u128().to_be_bytes());
      }
    }
    out
  }

  /// The id of the one node that holds this atom. Stable across builds,
  /// platforms and replicas: 80 bits of a BLAKE3 hash of
  /// [`canonical_bytes`](Self::canonical_bytes), with the ULID timestamp
  /// left at zero (see [`NodeId::is_value_addressed`]).
  pub fn node_id(&self) -> NodeId {
    let hash = blake3::hash(&self.canonical_bytes());
    let low: [u8; 16] = hash.as_bytes()[..16].try_into().unwrap();
    NodeId::from_u128(u128::from_le_bytes(low) & ((1 << 80) - 1))
  }

  /// A fresh formula condition node holding this atom. Its name is empty:
  /// formula nodes render from the atom and its referents.
  pub fn node_added(&self) -> Event {
    Event::NodeAdded {
      node: self.node_id(),
      kind: NodeKind::formula(self.clone()),
      name: String::new(),
      order_hint: 0.0,
    }
  }

  /// The events that make `from` require this atom: the edge, preceded by
  /// the formula node only when `graph` does not hold it yet. Requiring an
  /// atom that exists links to the existing node — deduplication by
  /// identity, with no index to consult.
  pub fn require(
    &self,
    graph: &Graph,
    from: NodeId,
    edge: EdgeId,
  ) -> Vec<Event> {
    let to = self.node_id();
    let mut events = Vec::with_capacity(2);
    if graph.node(to).is_none() {
      events.push(self.node_added());
    }
    events.push(Event::EdgeAdded {
      edge,
      kind: EdgeKind::Dependency,
      from,
      to,
    });
    events
  }

  /// Whether the atom holds under `facts`, when that could next change by
  /// time alone, and why. Referents and resource balances are read from
  /// `graph`; an atom whose referent is gone does not hold.
  pub fn eval(&self, graph: &Graph, facts: &Facts) -> Truth {
    let now = facts.now;
    match self {
      Atom::After { at } => {
        let at = at.instant(&facts.zone);
        let holds = now >= at;
        Truth::new(holds, (!holds).then_some(at), Explanation::Clock)
      }
      Atom::Before { at } => {
        let at = at.instant(&facts.zone);
        let holds = now < at;
        Truth::new(holds, holds.then_some(at), Explanation::Clock)
      }
      Atom::Within { schedule } => match graph.schedule(*schedule) {
        Some(s) => {
          let (holds, until) = schedule::truth(&s.spans, now, &facts.zone);
          Truth::new(holds, until, Explanation::Clock)
        }
        None => Truth::missing(),
      },
      Atom::Free { at_least } => {
        let Some(end) = facts.free_until else {
          return Truth::new(false, None, Explanation::Free { left: None });
        };
        let left = end.duration_since(now).as_mins().clamp(0, u32::MAX.into());
        let why = Explanation::Free {
          left: Some(left as Minutes),
        };
        // Holds strictly before the threshold, like `Before`, so it flips
        // at an instant and `Free { at_least: 0 }` means "free right now".
        let threshold = end
          .checked_sub(SignedDuration::from_mins((*at_least).into()))
          .unwrap_or(Timestamp::MIN);
        let holds = now < threshold;
        Truth::new(holds, holds.then_some(threshold), why)
      }
      Atom::At { place } => {
        if graph.place(*place).is_none() {
          return Truth::missing();
        }
        let holds = facts.places.iter().any(|p| graph.is_within(*p, *place));
        let mut here: Vec<PlaceId> = facts.places.iter().copied().collect();
        here.sort_unstable();
        Truth::new(holds, None, Explanation::Place { here })
      }
      Atom::Has { resource, at_least } => match graph.resource(*resource) {
        Some(r) => Truth::new(
          r.balance >= *at_least,
          None,
          Explanation::Balance { have: r.balance },
        ),
        None => Truth::missing(),
      },
      Atom::In { context } => {
        if graph.context(*context).is_none() {
          return Truth::missing();
        }
        let holds = facts.contexts.contains(context);
        Truth::new(holds, None, Explanation::Context)
      }
    }
  }
}

/// What an atom's evaluation found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Truth {
  /// Whether the atom holds.
  pub holds: bool,
  /// The earliest instant at which `holds` could change with no fact but
  /// the clock changing, or `None` if only another fact can change it.
  /// Truth is constant from now until then; it is exact for every atom
  /// except a schedule covering every moment of its look-ahead, which
  /// reports the end of the look-ahead instead.
  pub until: Option<Timestamp>,
  /// The facts behind `holds`, for the inspector to phrase alongside the
  /// atom ("Opens Thu 1 Oct", "Needs $50.00, have $32.10").
  pub why: Explanation,
}

impl Truth {
  fn new(holds: bool, until: Option<Timestamp>, why: Explanation) -> Self {
    Self { holds, until, why }
  }

  fn missing() -> Self {
    Self::new(false, None, Explanation::Missing)
  }
}

/// The facts an atom's truth came from, beyond the atom itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Explanation {
  /// The clock decides: [`Truth::holds`] and [`Truth::until`] say it all
  /// ("Opens in 3 days", "Closes at 17:00", "Closed").
  Clock,
  /// Declared free time: how many minutes are left, or `None` when no
  /// free-until is declared.
  Free {
    /// Whole minutes until free time ends.
    left: Option<Minutes>,
  },
  /// Where I declared I am (not the places those lie within).
  Place {
    /// The declared places, sorted.
    here: Vec<PlaceId>,
  },
  /// The resource's balance.
  Balance {
    /// How much I have.
    have: Amount,
  },
  /// Whether the context is active.
  Context,
  /// The referent the atom points at no longer exists.
  Missing,
}
