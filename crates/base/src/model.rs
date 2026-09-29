//! The domain model: nodes, edges, and quests (PLAN §2).
//!
//! All entities live in one global, flat namespace. Edges are first-class
//! and connect any node to any node; quests *claim* nodes but own nothing.

use serde::{Deserialize, Serialize};

use crate::{
  formula::Atom,
  ids::{EdgeId, NodeId, QuestId},
};

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

  /// A formula condition holding `atom`. Its stored bit stays `false`:
  /// the atom decides its truth, in [`Derived`](crate::Derived).
  pub fn formula(atom: Atom) -> Self {
    Self::Condition {
      satisfied: false,
      source:    ConditionSource::Formula { atom },
    }
  }

  /// The atom of a formula condition.
  pub fn atom(&self) -> Option<&Atom> {
    match self {
      Self::Condition {
        source: ConditionSource::Formula { atom },
        ..
      } => Some(atom),
      _ => None,
    }
  }

  /// Whether a quest can claim this node. A formula condition is shared by
  /// everything that asks the same thing of the world, so it belongs to no
  /// quest of its own; it shows in a lens only when claimed work requires
  /// it.
  pub fn claimable(&self) -> bool { self.atom().is_none() }

  /// The stored completion/satisfaction bit: a completed task or a
  /// satisfied manual condition. Always `false` for a formula condition,
  /// whose truth depends on facts; gating reads
  /// [`Derived::is_satisfied`](crate::Derived::is_satisfied).
  pub fn is_satisfied(&self) -> bool {
    match self {
      Self::Task { completed } => *completed,
      Self::Condition { satisfied, .. } => *satisfied,
    }
  }

  /// Set the completion/satisfaction bit regardless of variant, returning
  /// the previous value. A no-op on a formula condition: no click sets it.
  pub fn set_satisfied(&mut self, value: bool) -> bool {
    match self {
      Self::Task { completed } => core::mem::replace(completed, value),
      Self::Condition {
        source: ConditionSource::Formula { .. },
        ..
      } => false,
      Self::Condition { satisfied, .. } => core::mem::replace(satisfied, value),
    }
  }
}

/// How a [`Condition`](NodeKind::Condition)'s value is produced.
///
/// The enum is non-exhaustive so further sources slot in without a schema
/// migration (PLAN §2).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "source", rename_all = "snake_case")]
#[non_exhaustive]
pub enum ConditionSource {
  /// The user toggles satisfaction by hand.
  Manual,
  /// Computed from facts by an atom (plans/formula-conditions.md). The
  /// node's id is the atom's [`node_id`](Atom::node_id); it has no
  /// requirements and is never actionable.
  Formula {
    /// The predicate that decides the condition.
    atom: Atom,
  },
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
/// Dependency is the only kind; there are deliberately no subtasks (PLAN
/// §7.5). The enum is non-exhaustive so further kinds can be added later
/// without a schema change, and stored edges already carry their kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EdgeKind {
  /// A plain dependency: the source needs the target.
  Dependency,
}

/// A first-class directed edge. `from` requires `to` — always read in that
/// direction (PLAN §2).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Edge {
  /// Stable identity, needed for the event log and reordering.
  pub id:   EdgeId,
  /// The edge's kind (only [`EdgeKind::Dependency`] exists).
  pub kind: EdgeKind,
  /// The dependent.
  pub from: NodeId,
  /// The requirement.
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
