//! The selected node, as the inspector shows it, and changing the
//! selection.

use base::{
  Atom, ContextId, Derived, EdgeId, Facts, Graph, NodeId, NodeKind, NodeState,
  PlaceId, QuestId, ResourceId, ScheduleId,
};
use jiff::civil::Date;

use super::AppState;
use crate::{camera::CameraRequest, formula::describe, scene::Category};

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
  /// For a formula condition, what stands in for its name, and the
  /// referent its atom points at, to edit in place.
  pub formula:      Option<FormulaInfo>,
}

/// A formula condition, as the inspector shows it: a label derived from
/// its atom rather than an editable name, and the thing it is about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormulaInfo {
  /// The atom's label: "At Home".
  pub title:    String,
  /// What it is about, for its glyph.
  pub glyph:    Category,
  /// The referent to edit, if the atom has one.
  pub referent: ReferentInfo,
}

/// The referent behind a formula condition, for its card in the inspector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ReferentInfo {
  /// A date: nothing to edit but the atom itself.
  Date,
  /// A place, and whether I am there.
  Place {
    /// Which place.
    id:   PlaceId,
    /// Its name.
    name: String,
    /// Whether I declared I am there (or somewhere within it).
    here: bool,
  },
  /// A resource and its balance.
  Resource {
    /// Which resource.
    id:      ResourceId,
    /// Its name.
    name:    String,
    /// How much I have, in words ("$320.00").
    balance: String,
  },
  /// A schedule and its windows.
  Schedule {
    /// Which schedule.
    id:    ScheduleId,
    /// Its name.
    name:  String,
    /// Each window, in words.
    spans: Vec<String>,
  },
  /// A context, and whether it is on.
  Context {
    /// Which context.
    id:   ContextId,
    /// Its name.
    name: String,
    /// Whether I declared it active.
    on:   bool,
  },
  /// Declared free time.
  Free {
    /// When it ends, in words, if declared.
    until: Option<String>,
  },
  /// The referent was deleted.
  Missing,
}

/// A node the reason names, with a sentence on why it matters when it is a
/// formula condition ("Opens Thu 1 Oct, in 3 days").
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Named {
  /// The node.
  pub node: NodeId,
  /// Its name.
  pub name: String,
  /// Why it holds or not, for a formula condition.
  pub why:  Option<String>,
}

/// Why the selected node is in its state, for the inspector's reason line.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Reason {
  /// Ready: this many requirements, all met (possibly none).
  AllMet(usize),
  /// Blocked (or a condition waiting) on these unmet requirements.
  WaitingOn(Vec<Named>),
  /// In a cycle with these nodes; never ready until it is broken.
  CycleWith(Vec<Named>),
  /// A completed task.
  Completed,
  /// A satisfied condition.
  Satisfied,
  /// A condition with nothing unmet, waiting to be set satisfied.
  AwaitingSatisfaction,
  /// A formula condition: why its atom holds or not, in a sentence ("Opens
  /// Thu 1 Oct, in 3 days").
  Computed(String),
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
  /// Nothing to press: a date follows the clock, not clicks.
  Automatic,
  /// Declare that I am at `place`, or (`here`) that I have left.
  Here {
    /// The place.
    place: PlaceId,
    /// Whether I am there now.
    here:  bool,
  },
  /// Turn a context on or off.
  Toggle {
    /// The context.
    context: ContextId,
    /// Whether it is on now.
    on:      bool,
  },
  /// Go to the balance field.
  SetBalance,
  /// Go to the field that adds a window.
  EditSchedule,
  /// Go to the free time field, in the Now tray.
  SetFreeTime,
}

impl EdgeRow {
  /// The row for `edge`, describing the node at its `other` end.
  fn new(
    graph: &Graph,
    derived: &Derived,
    today: Date,
    edge: EdgeId,
    other: NodeId,
  ) -> Self {
    Self {
      edge,
      name: graph
        .node(other)
        .map(|n| describe::node_name(graph, n, today))
        .unwrap_or_default(),
      state: derived.state(other).unwrap_or(NodeState::Blocked),
      other,
    }
  }
}

impl Reason {
  /// Why `node`, in `state`, is in it, as derived under `facts`.
  fn for_node(
    graph: &Graph,
    derived: &Derived,
    facts: &Facts,
    node: NodeId,
    state: NodeState,
  ) -> Self {
    let why = |n: NodeId| {
      let atom = graph.node(n)?.kind.atom()?;
      let truth = derived.truth(n)?;
      Some(describe::truth_sentence(
        graph,
        atom,
        truth,
        facts.now,
        &facts.zone,
      ))
    };
    if let Some(sentence) = why(node) {
      return Reason::Computed(sentence);
    }
    let today = facts.zone.to_datetime(facts.now).date();
    // By name, for a list that does not reshuffle as the graph changes.
    let named = |ids: Vec<NodeId>| -> Vec<Named> {
      let mut v: Vec<Named> = ids
        .into_iter()
        .filter_map(|n| {
          graph.node(n).map(|node| Named {
            node: n,
            name: describe::node_name(graph, node, today),
            why:  why(n),
          })
        })
        .collect();
      v.sort_by(|a, b| a.name.cmp(&b.name));
      v
    };
    let unmet = || {
      named(
        graph
          .requirements_of(node)
          .map(|e| e.to)
          .filter(|t| !derived.is_satisfied(*t))
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
      Reason::Computed(sentence) => sentence.clone(),
    }
  }

  /// Whether the reason is a problem to fix (a cycle), not just a state.
  pub fn is_alert(&self) -> bool { matches!(self, Reason::CycleWith(_)) }

  /// The other nodes that are the reason, by name, if any.
  pub fn nodes(&self) -> &[Named] {
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
      Primary::Automatic => "Automatic",
      Primary::Here { here: false, .. } => "I'm here",
      Primary::Here { here: true, .. } => "I've left",
      Primary::Toggle { on: false, .. } => "Turn on",
      Primary::Toggle { on: true, .. } => "Turn off",
      Primary::SetBalance => "Set balance\u{2026}",
      Primary::EditSchedule => "Edit schedule\u{2026}",
      Primary::SetFreeTime => "Set free time\u{2026}",
    }
  }

  /// Whether the button can be pressed.
  pub fn enabled(self) -> bool {
    !matches!(
      self,
      Primary::Complete { enabled: false } | Primary::Automatic
    )
  }

  /// The primary action for a formula condition about `referent`: one that
  /// changes the fact it reads.
  fn for_formula(referent: &ReferentInfo) -> Self {
    match *referent {
      ReferentInfo::Place { id, here, .. } => Primary::Here { place: id, here },
      ReferentInfo::Context { id, on, .. } => {
        Primary::Toggle { context: id, on }
      }
      ReferentInfo::Resource { .. } => Primary::SetBalance,
      ReferentInfo::Schedule { .. } => Primary::EditSchedule,
      ReferentInfo::Free { .. } => Primary::SetFreeTime,
      ReferentInfo::Date | ReferentInfo::Missing => Primary::Automatic,
    }
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
    let cached = self.derivations(&store);
    let derived = &cached.derived;
    let today = self.today();

    // `requirements_of` walks outgoing edges (what this node needs) and
    // `dependents_of` incoming ones (what needs this node); either way the
    // row describes the node at the *other* end.
    let mut requirements: Vec<EdgeRow> = graph
      .requirements_of(id)
      .map(|e| EdgeRow::new(graph, derived, today, e.id, e.to))
      .collect();
    let mut dependents: Vec<EdgeRow> = graph
      .dependents_of(id)
      .map(|e| EdgeRow::new(graph, derived, today, e.id, e.from))
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

    let formula = node.kind.atom().map(|atom| FormulaInfo {
      title:    describe::atom_label(graph, atom, today),
      glyph:    Category::of(atom),
      referent: self.referent_info(graph, atom),
    });
    let primary = match &formula {
      Some(f) => Primary::for_formula(&f.referent),
      None => Primary::for_node(&node.kind, state),
    };

    Some(SelectedInfo {
      state,
      is_task: matches!(node.kind, NodeKind::Task { .. }),
      reason: Reason::for_node(graph, derived, &cached.facts, id, state),
      primary,
      formula,
      quests,
      claimed,
      requirements,
      dependents,
    })
  }

  /// What `atom` points at, as its card shows it.
  fn referent_info(&self, graph: &Graph, atom: &Atom) -> ReferentInfo {
    let today = self.today();
    let missing = ReferentInfo::Missing;
    match atom {
      Atom::After { .. } | Atom::Before { .. } => ReferentInfo::Date,
      Atom::At { place } => {
        graph
          .place(*place)
          .map_or(missing, |p| ReferentInfo::Place {
            id:   p.id,
            name: p.name.clone(),
            here: self.place().is_some_and(|at| graph.is_within(at, p.id)),
          })
      }
      Atom::Has { resource, .. } => {
        graph
          .resource(*resource)
          .map_or(missing, |r| ReferentInfo::Resource {
            id:      r.id,
            name:    r.name.clone(),
            balance: describe::amount(r, r.balance),
          })
      }
      Atom::Within { schedule } => {
        graph
          .schedule(*schedule)
          .map_or(missing, |s| ReferentInfo::Schedule {
            id:    s.id,
            name:  s.name.clone(),
            spans: s.spans.iter().map(|sp| describe::span(sp, today)).collect(),
          })
      }
      Atom::In { context } => {
        graph
          .context(*context)
          .map_or(missing, |c| ReferentInfo::Context {
            id:   c.id,
            name: c.name.clone(),
            on:   self.active_contexts().contains(&c.id),
          })
      }
      Atom::Free { .. } => ReferentInfo::Free {
        until: self.free_summary(),
      },
    }
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
    self.selected_copy = None;
    self.source = None;
    self.reset_drafts();
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
    let store = self.lock();
    let graph = store.graph();
    graph
      .node(id)
      .map(|n| describe::node_name(graph, n, self.today()))
  }
}
