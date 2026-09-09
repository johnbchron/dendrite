//! `base` — the pure domain model and graph algorithms for Neutron.
//!
//! One global, flat, fully cross-linkable graph of [`Node`]s joined by
//! first-class [`Edge`]s; [`Quest`]s are lenses that *claim* nodes without
//! owning them. This crate is deliberately I/O- and UI-free: it holds the
//! data model, the derived-state algorithms (readiness, Tarjan cycle
//! detection, quest scope + actionable queries), and the [`Event`] log
//! reducer with inverse generation for undo (PLAN §4).

pub mod derive;
pub mod event;
pub mod graph;
pub mod ids;
pub mod model;
pub mod quest;

pub use derive::{Derived, NodeState, cyclic_nodes};
pub use event::{Event, apply_batch};
pub use graph::Graph;
pub use ids::{EdgeId, EventId, NodeId, QuestId};
pub use model::{ConditionSource, Edge, EdgeKind, Node, NodeKind, Quest};
pub use quest::{QuestScope, actionable, claiming_quests, scope};
