//! `base` — the pure domain model and graph algorithms for Dendrite.
//!
//! One global, flat, fully cross-linkable graph of [`Node`]s joined by
//! first-class [`Edge`]s; [`Quest`]s are lenses that *claim* nodes without
//! owning them. This crate is deliberately I/O- and UI-free: it holds the
//! data model, the derived-state algorithms (readiness, Tarjan cycle
//! detection, quest scope + actionable queries), and the [`Event`] log
//! reducer with inverse generation for undo (PLAN §4).
//!
//! Formula conditions ([`Atom`]) are evaluated against [`Facts`] passed in
//! by the caller: `base` never reads a clock. [`jiff`] is re-exported for
//! the time types facts are made of.

pub mod completed;
pub mod derive;
pub mod event;
pub mod formula;
pub mod graph;
pub mod ids;
pub mod model;
pub mod prune;
pub mod quest;
pub mod referent;

pub use completed::{Completed, completed};
pub use derive::{Derived, NodeState, cycle_peers, cyclic_nodes};
pub use event::{Event, apply_batch, apply_group};
pub use formula::{
  Amount, Atom, Explanation, Facts, Minutes, Moment, TimeOfDay, Truth,
  WeekdaySet,
};
pub use graph::Graph;
pub use ids::{
  ContextId, EdgeId, EventId, NodeId, PlaceId, QuestId, ResourceId, ScheduleId,
};
pub use jiff;
pub use model::{ConditionSource, Edge, EdgeKind, Node, NodeKind, Quest};
pub use prune::prune;
pub use quest::{
  QuestScope, actionable, all_quests_scope, claiming_quests, scope,
};
pub use referent::{Context, Place, Resource, Schedule, Span, Unit};
