//! The selected node, as the inspector shows it, and changing the
//! selection.

use base::{Derived, EdgeId, Graph, NodeId, NodeKind, NodeState, QuestId};

use super::AppState;
use crate::canvas::CameraRequest;

/// One edge incident to the selected node, as the inspector shows it. Carries
/// the [`EdgeId`] so a row can delete the edge it stands for.
pub struct EdgeRow {
  /// The edge this row stands for.
  pub edge:  EdgeId,
  /// The node at the *other* end of the edge.
  pub other: NodeId,
  /// Its name.
  pub name:  String,
  /// Its derived state.
  pub state: NodeState,
}

/// A summary of the selected node for the inspector. The *name* is not here:
/// the inspector's title is an editable field fed from
/// [`AppState::name_draft`], which `select` and undo/redo keep in step with the
/// graph.
pub struct SelectedInfo {
  /// Human-readable derived state.
  pub state:        NodeState,
  /// Whether the node is a task (vs. a condition).
  pub is_task:      bool,
  /// Why the node is in its state.
  pub reason:       Reason,
  /// The one action the inspector leads with.
  pub primary:      Primary,
  /// Every quest that claims the node, by name.
  pub quests:       Vec<(QuestId, String)>,
  /// Whether the active quest claims the node; `None` in the global view.
  pub claimed:      Option<bool>,
  /// Edges to the things this node requires.
  pub requirements: Vec<EdgeRow>,
  /// Edges from the things that require this node — the other direction,
  /// which answers "what does finishing this unblock?".
  pub dependents:   Vec<EdgeRow>,
}

/// Why the selected node is in its state, for the inspector's reason line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
  /// Ready: this many requirements, all met (possibly none).
  AllMet(usize),
  /// Blocked (or a condition waiting) on these unmet requirements.
  WaitingOn(Vec<(NodeId, String)>),
  /// In a cycle with these nodes; never ready until it is broken.
  CycleWith(Vec<(NodeId, String)>),
  /// A completed task.
  Completed,
  /// A satisfied condition.
  Satisfied,
  /// A condition with nothing unmet, waiting to be set satisfied.
  AwaitingSatisfaction,
}

/// The inspector's primary action for the selected node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Primary {
  /// Mark a task complete. Offered but disabled unless it is Ready: PLAN §5
  /// only lets Ready tasks be completed.
  Complete {
    /// Whether the task is Ready.
    enabled: bool,
  },
  /// Reopen a completed task.
  Reopen,
  /// Mark a condition satisfied.
  Satisfy,
  /// Clear a condition's satisfaction.
  Unsatisfy,
}

impl EdgeRow {
  /// The row for `edge`, describing the node at its `other` end.
  fn new(
    graph: &Graph,
    derived: &Derived,
    edge: EdgeId,
    other: NodeId,
  ) -> Self {
    Self {
      edge,
      name: graph
        .node(other)
        .map(|n| n.name.clone())
        .unwrap_or_default(),
      state: derived.state(other).unwrap_or(NodeState::Blocked),
      other,
    }
  }
}

impl Reason {
  /// Why `node`, in `state`, is in it.
  fn for_node(graph: &Graph, node: NodeId, state: NodeState) -> Self {
    // By name, for a list that does not reshuffle as the graph changes.
    let named = |ids: Vec<NodeId>| -> Vec<(NodeId, String)> {
      let mut v: Vec<(NodeId, String)> = ids
        .into_iter()
        .filter_map(|n| graph.node(n).map(|node| (n, node.name.clone())))
        .collect();
      v.sort_by(|a, b| a.1.cmp(&b.1));
      v
    };
    let unmet = || {
      named(
        graph
          .requirements_of(node)
          .map(|e| e.to)
          .filter(|t| !graph.is_satisfied(*t))
          .collect(),
      )
    };
    match state {
      NodeState::Completed => Reason::Completed,
      NodeState::Satisfied => Reason::Satisfied,
      NodeState::Cyclic => {
        Reason::CycleWith(named(base::cycle_peers(graph, node)))
      }
      NodeState::Ready => Reason::AllMet(graph.requirements_of(node).count()),
      NodeState::Blocked => Reason::WaitingOn(unmet()),
      NodeState::Pending => match unmet() {
        waiting if waiting.is_empty() => Reason::AwaitingSatisfaction,
        waiting => Reason::WaitingOn(waiting),
      },
    }
  }

  /// The reason as one sentence, for the inspector.
  pub fn sentence(&self) -> String {
    let count = |n: usize, one: &str, many: &str| {
      if n == 1 {
        one.to_string()
      } else {
        format!("{n} {many}")
      }
    };
    match self {
      Reason::AllMet(0) => "Nothing required: ready to do.".to_string(),
      Reason::AllMet(n) => format!(
        "{} met.",
        count(*n, "Its one requirement", "requirements, all")
      ),
      Reason::WaitingOn(_) => "Waiting on:".to_string(),
      Reason::CycleWith(_) => {
        "In a cycle with these; remove an edge to break it:".to_string()
      }
      Reason::Completed => "Completed.".to_string(),
      Reason::Satisfied => "Satisfied.".to_string(),
      Reason::AwaitingSatisfaction => {
        "Nothing unmet: waiting to be marked satisfied.".to_string()
      }
    }
  }

  /// Whether the reason is a problem to fix (a cycle), not just a state.
  pub fn is_alert(&self) -> bool { matches!(self, Reason::CycleWith(_)) }

  /// The other nodes that are the reason, by name, if any.
  pub fn nodes(&self) -> &[(NodeId, String)] {
    match self {
      Reason::WaitingOn(nodes) | Reason::CycleWith(nodes) => nodes,
      _ => &[],
    }
  }
}

impl Primary {
  /// The button's label.
  pub fn label(self) -> &'static str {
    match self {
      Primary::Complete { .. } => "Mark complete",
      Primary::Reopen => "Reopen",
      Primary::Satisfy => "Mark satisfied",
      Primary::Unsatisfy => "Unsatisfy",
    }
  }

  /// Whether the button can be pressed.
  pub fn enabled(self) -> bool {
    !matches!(self, Primary::Complete { enabled: false })
  }

  /// The primary action for a node of `kind` in `state`.
  fn for_node(kind: &NodeKind, state: NodeState) -> Self {
    match (kind, state) {
      (NodeKind::Task { completed: true }, _) => Primary::Reopen,
      (NodeKind::Task { .. }, state) => Primary::Complete {
        enabled: state == NodeState::Ready,
      },
      (
        NodeKind::Condition {
          satisfied: true, ..
        },
        _,
      ) => Primary::Unsatisfy,
      (NodeKind::Condition { .. }, _) => Primary::Satisfy,
    }
  }
}

impl AppState {
  /// Details of the selected node for the inspector.
  pub fn selected_info(&self) -> Option<SelectedInfo> {
    let id = self.selected?;
    let store = self.lock();
    let graph = store.graph();
    let node = graph.node(id)?;
    let derived = &self.derivations(&store).derived;

    // `requirements_of` walks outgoing edges (what this node needs) and
    // `dependents_of` incoming ones (what needs this node); either way the
    // row describes the node at the *other* end.
    let mut requirements: Vec<EdgeRow> = graph
      .requirements_of(id)
      .map(|e| EdgeRow::new(graph, derived, e.id, e.to))
      .collect();
    let mut dependents: Vec<EdgeRow> = graph
      .dependents_of(id)
      .map(|e| EdgeRow::new(graph, derived, e.id, e.from))
      .collect();
    // Adjacency order is an implementation detail; sort so the panel does not
    // reshuffle as edges come and go.
    requirements.sort_by(|a, b| a.name.cmp(&b.name));
    dependents.sort_by(|a, b| a.name.cmp(&b.name));

    let state = derived.state(id).unwrap_or(NodeState::Blocked);
    let mut quests: Vec<(QuestId, String)> = base::claiming_quests(graph, id)
      .into_iter()
      .filter_map(|q| graph.quest(q).map(|quest| (q, quest.name.clone())))
      .collect();
    quests.sort_by(|a, b| a.1.cmp(&b.1));
    let claimed = self
      .active_quest
      .map(|q| quests.iter().any(|(claimer, _)| *claimer == q));

    Some(SelectedInfo {
      state,
      is_task: matches!(node.kind, NodeKind::Task { .. }),
      reason: Reason::for_node(graph, id, state),
      primary: Primary::for_node(&node.kind, state),
      quests,
      claimed,
      requirements,
      dependents,
    })
  }

  /// Select (or clear) the current node; resets the rename draft and disarms
  /// any pending requirement link.
  pub fn select(&mut self, node: Option<NodeId>) {
    self.cancel_link();
    self.live_edit = None;
    self.more_open = false;
    self.quests_open = false;
    if let Some(node) = node {
      self.recent.remember(node);
    }
    self.selected = node;
    self.name_draft = {
      let store = self.lock();
      node
        .and_then(|id| store.graph().node(id))
        .map(|n| n.name.clone())
        .unwrap_or_default()
    };
  }

  /// Select `node` and bring it into view on the canvas: the "go to" used
  /// by lists that name nodes.
  pub fn go_to(&mut self, node: NodeId) {
    self.select(Some(node));
    self.aim(CameraRequest::Reveal(node));
  }

  /// The name of the node new nodes would attach to, for the create
  /// buttons' tooltips.
  pub fn attach_point(&self) -> Option<String> {
    let id = self.selected?;
    self.lock().graph().node(id).map(|n| n.name.clone())
  }
}
