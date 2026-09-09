//! The domain model: nodes, edges, and quests (PLAN §2).
//!
//! All entities live in one global, flat namespace. Edges are first-class
//! and connect any node to any node; quests *claim* nodes but own nothing.

use serde::{Deserialize, Serialize};

use crate::ids::{EdgeId, NodeId, QuestId};

/// A node is either a [`Task`](NodeKind::Task) the user performs or a
/// [`Condition`](NodeKind::Condition) that becomes satisfied.
///
/// The mutable completion/satisfaction bit lives *inside* the kind because
/// the two variants carry different payloads; structural data common to
/// both (id, name, ordering) lives on [`Node`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeKind {
  /// A unit of work the user completes.
  Task {
    /// Whether the task has been marked done.
    completed: bool,
  },
  /// A fact that is either satisfied or pending.
  Condition {
    /// Whether the condition currently holds.
    satisfied: bool,
    /// How the condition's value is determined.
    source:    ConditionSource,
  },
}

impl NodeKind {
  /// A freshly-added, not-yet-done task.
  pub fn task() -> Self { Self::Task { completed: false } }

  /// A freshly-added, pending, manually-controlled condition.
  pub fn condition() -> Self {
    Self::Condition {
      satisfied: false,
      source:    ConditionSource::Manual,
    }
  }

  /// Whether this node counts as satisfied for the purposes of gating
  /// downstream work: a completed task or a satisfied condition.
  pub fn is_satisfied(&self) -> bool {
    match self {
      Self::Task { completed } => *completed,
      Self::Condition { satisfied, .. } => *satisfied,
    }
  }

  /// Set the completion/satisfaction bit regardless of variant, returning
  /// the previous value.
  pub fn set_satisfied(&mut self, value: bool) -> bool {
    match self {
      Self::Task { completed } => core::mem::replace(completed, value),
      Self::Condition { satisfied, .. } => core::mem::replace(satisfied, value),
    }
  }
}

/// How a [`Condition`](NodeKind::Condition)'s value is produced.
///
/// Only [`Manual`](ConditionSource::Manual) ships in v1; the enum is
/// non-exhaustive so auto-evaluated sources slot in without a schema
/// migration (PLAN §2, §9).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConditionSource {
  /// The user toggles satisfaction by hand.
  Manual,
}

/// A single node in the global graph.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Node {
  /// Stable identity; the *only* way to look a node up (names are free-form
  /// and non-unique, PLAN §7.1).
  pub id:         NodeId,
  /// Free-form display text.
  pub name:       String,
  /// Task-or-condition payload.
  pub kind:       NodeKind,
  /// User-supplied within-level ordering hint that seeds layout crossing
  /// minimization so a chosen order survives relayout (PLAN §5).
  pub order_hint: f64,
}

impl Node {
  /// Construct a node with the given identity and payload.
  pub fn new(
    id: NodeId,
    name: impl Into<String>,
    kind: NodeKind,
    order_hint: f64,
  ) -> Self {
    Self {
      id,
      name: name.into(),
      kind,
      order_hint,
    }
  }
}

/// The semantic flavour of an [`Edge`].
///
/// Both variants gate identically ("requires all targets satisfied"); the
/// distinction is purely visual/organizational (PLAN §2). The enum is
/// non-exhaustive so further kinds can be added later (PLAN §7.5).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EdgeKind {
  /// A plain dependency: the source needs the target.
  Dependency,
  /// A subtask relationship: the parent needs its child. Gates like a
  /// dependency; drawn differently.
  Subtask,
}

/// A first-class directed edge. `from` requires `to` — always read in that
/// direction (PLAN §2).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edge {
  /// Stable identity, needed for the event log and reordering.
  pub id:   EdgeId,
  /// Dependency vs. subtask.
  pub kind: EdgeKind,
  /// The dependent / parent.
  pub from: NodeId,
  /// The requirement / child.
  pub to:   NodeId,
}

impl Edge {
  /// Construct an edge.
  pub fn new(id: EdgeId, kind: EdgeKind, from: NodeId, to: NodeId) -> Self {
    Self { id, kind, from, to }
  }
}

/// A quest is a *lens*: it claims a set of nodes as an epic without owning
/// them. Claims are many-to-many (PLAN §1, §2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quest {
  /// Stable identity.
  pub id:     QuestId,
  /// Free-form display name.
  pub name:   String,
  /// The claim set — membership, not ownership. Kept from the old `roots`
  /// field but reinterpreted (PLAN §2, §7).
  pub claims: std::collections::HashSet<NodeId>,
}

impl Quest {
  /// Construct an empty quest.
  pub fn new(id: QuestId, name: impl Into<String>) -> Self {
    Self {
      id,
      name: name.into(),
      claims: std::collections::HashSet::new(),
    }
  }
}
