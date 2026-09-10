//! Application state and the command layer that turns UI gestures into
//! [`base::Event`]s committed through the [`db::Store`] (PLAN §3, §4).
//!
//! `AppState` owns the store (the persistent global graph) and the ephemeral
//! view state — the current selection, the active quest lens, and the rename
//! draft. Everything the canvas and side panel render is derived here from
//! the store on demand.
//!
//! The store is wrapped in a [`Mutex`] because Xilem's `WidgetView` bound is
//! `Send + Sync` and rusqlite's `Connection` is `!Sync`. The app is
//! single-threaded, so the lock is uncontended; each read method takes it
//! exactly once (the `std` mutex is not re-entrant).

use std::sync::{Mutex, MutexGuard};

use base::{
  Derived, EdgeId, EdgeKind, Event, NodeId, NodeKind, NodeState, QuestId,
};
use db::Store;
use layout::LayoutConfig;

use crate::canvas::{CanvasScene, RenderEdge, RenderNode};

/// The whole application's state.
pub struct AppState {
  store:            Mutex<Store>,
  /// Currently selected node, if any.
  pub selected:     Option<NodeId>,
  /// Active quest lens; `None` means the global "all nodes" view (PLAN §5).
  pub active_quest: Option<QuestId>,
  /// Editable name buffer for the selected node.
  pub name_draft:   String,
  /// Bumped to ask the canvas to refit/centre the graph.
  recenter_epoch:   u64,
  /// Current width of the side panel, in logical pixels.
  panel_width:      f64,
  /// Panel width when the current divider drag started, so drags measure
  /// against a fixed anchor instead of accumulating.
  panel_width_base: f64,
  /// When set, the next canvas click picks a requirement target for the
  /// selection instead of changing the selection.
  linking:          bool,
  /// Filter text for the requirement picker — the fallback path for targets
  /// that are not on the canvas (e.g. outside the active quest's scope).
  pub link_filter:  String,
  /// Whether the quest switcher is expanded in the panel's lens bar.
  picker_open:      bool,
}

/// Default width of the side panel, in logical pixels.
const PANEL_WIDTH: f64 = 380.0;
/// How narrow and how wide the side panel may be dragged.
const PANEL_MIN: f64 = 280.0;
const PANEL_MAX: f64 = 680.0;
/// Most rows the requirement picker will ever show. The panel must not grow
/// with the graph; anything beyond this is narrowed with the filter instead.
const LINK_PICKER_MAX: usize = 6;
/// Most rows the pinned actionable list will ever show, for the same reason.
const ACTIONABLE_MAX: usize = 6;

/// One edge incident to the selected node, as the side panel shows it. Carries
/// the [`EdgeId`] so a row can delete the edge it stands for.
pub struct EdgeRow {
  /// The edge this row stands for.
  pub edge:      EdgeId,
  /// Name of the node at the *other* end of the edge.
  pub name:      String,
  /// Whether that node is satisfied (completed task / satisfied condition).
  pub satisfied: bool,
}

/// A summary of the selected node for the side panel. The *name* is not here:
/// the inspector's title is an editable field fed from
/// [`AppState::name_draft`], which `select` and undo/redo keep in step with the
/// graph.
pub struct SelectedInfo {
  /// Human-readable derived state.
  pub state:        NodeState,
  /// Whether the node is a task (vs. a condition).
  pub is_task:      bool,
  /// Edges to the things this node requires.
  pub requirements: Vec<EdgeRow>,
  /// Edges from the things that require this node — the other direction,
  /// which answers "what does finishing this unblock?".
  pub dependents:   Vec<EdgeRow>,
}

impl AppState {
  /// Open (or create) the store at `path` and seed demo data if empty.
  pub fn new(store: Store) -> Self {
    let empty = store.graph().node_count() == 0;
    let mut state = Self {
      store:            Mutex::new(store),
      selected:         None,
      active_quest:     None,
      name_draft:       String::new(),
      recenter_epoch:   0,
      panel_width:      PANEL_WIDTH,
      panel_width_base: PANEL_WIDTH,
      linking:          false,
      link_filter:      String::new(),
      picker_open:      false,
    };
    if empty {
      state.seed_demo();
    }
    state
  }

  fn lock(&self) -> MutexGuard<'_, Store> {
    self.store.lock().expect("store mutex poisoned")
  }

  // --- derived views ----------------------------------------------------

  /// Build the paint scene for the canvas, honouring the active quest lens.
  pub fn scene(&self) -> CanvasScene {
    let store = self.lock();
    let graph = store.graph();
    let derived = Derived::compute(graph);
    let lay = layout::layout(graph, &LayoutConfig::default());

    // Which nodes are visible, and which are only pulled in (dimmed)?
    let (visible, claimed): (
      std::collections::HashSet<NodeId>,
      std::collections::HashSet<NodeId>,
    ) = match self.active_quest {
      Some(q) => {
        let s = base::scope(graph, q);
        (s.all().collect(), s.claimed.clone())
      }
      None => (graph.nodes().map(|n| n.id).collect(), Default::default()),
    };

    let scoped = self.active_quest.is_some();
    let mut nodes = Vec::new();
    for node in graph.nodes() {
      if !visible.contains(&node.id) {
        continue;
      }
      let center = lay.pos(node.id).unwrap_or(layout::Pos { x: 0.0, y: 0.0 });
      nodes.push(RenderNode {
        id: node.id,
        center,
        label: node.name.clone(),
        kind: node.kind.clone(),
        state: derived.state(node.id).unwrap_or(NodeState::Blocked),
        selected: self.selected == Some(node.id),
        dimmed: scoped && !claimed.contains(&node.id),
      });
    }

    let mut edges = Vec::new();
    for edge in graph.edges() {
      if !visible.contains(&edge.from) || !visible.contains(&edge.to) {
        continue;
      }
      let (Some(from), Some(to)) = (lay.pos(edge.from), lay.pos(edge.to))
      else {
        continue;
      };
      edges.push(RenderEdge {
        from,
        to,
        kind: edge.kind,
        reversed: lay.is_reversed(edge.id),
      });
    }

    CanvasScene { nodes, edges }
  }

  /// Details of the selected node for the side panel.
  pub fn selected_info(&self) -> Option<SelectedInfo> {
    let id = self.selected?;
    let store = self.lock();
    let graph = store.graph();
    let node = graph.node(id)?;
    let derived = Derived::compute(graph);

    // `requirements_of` walks outgoing edges (what this node needs) and
    // `dependents_of` incoming ones (what needs this node); either way the
    // row describes the node at the *other* end.
    let row = |edge: EdgeId, other: NodeId| EdgeRow {
      edge,
      name: graph
        .node(other)
        .map(|n| n.name.clone())
        .unwrap_or_default(),
      satisfied: graph.is_satisfied(other),
    };
    let mut requirements: Vec<EdgeRow> =
      graph.requirements_of(id).map(|e| row(e.id, e.to)).collect();
    let mut dependents: Vec<EdgeRow> =
      graph.dependents_of(id).map(|e| row(e.id, e.from)).collect();
    // Adjacency order is an implementation detail; sort so the panel does not
    // reshuffle as edges come and go.
    requirements.sort_by(|a, b| a.name.cmp(&b.name));
    dependents.sort_by(|a, b| a.name.cmp(&b.name));

    Some(SelectedInfo {
      state: derived.state(id).unwrap_or(NodeState::Blocked),
      is_task: matches!(node.kind, NodeKind::Task { .. }),
      requirements,
      dependents,
    })
  }

  /// The current recenter epoch handed to the canvas view.
  pub fn recenter_epoch(&self) -> u64 { self.recenter_epoch }

  /// Ask the canvas to refit/centre the whole graph on the next frame.
  pub fn recenter(&mut self) { self.recenter_epoch += 1; }

  /// The side panel's current width in logical pixels.
  pub fn panel_width(&self) -> f64 { self.panel_width }

  /// Anchor a divider drag at the current panel width.
  pub fn begin_panel_resize(&mut self) {
    self.panel_width_base = self.panel_width;
  }

  /// Resize the panel from a divider drag. `dx` is the pointer's total travel
  /// since the press, so dragging left (negative) widens the panel.
  pub fn resize_panel(&mut self, dx: f64) {
    self.panel_width = (self.panel_width_base - dx).clamp(PANEL_MIN, PANEL_MAX);
  }

  /// Whether the canvas is armed to pick a requirement target.
  pub fn is_linking(&self) -> bool { self.linking }

  /// Arm the canvas: the next node click adds a requirement to the selection
  /// rather than moving the selection. No-op without a selection.
  pub fn begin_link(&mut self) {
    self.linking = self.selected.is_some();
    self.link_filter.clear();
  }

  /// Disarm without linking anything.
  pub fn cancel_link(&mut self) {
    self.linking = false;
    self.link_filter.clear();
  }

  /// A click on the canvas. While armed this consumes the click to build a
  /// requirement edge, keeping the selection put so several can be added in a
  /// row; otherwise it just moves the selection.
  pub fn canvas_click(&mut self, node: Option<NodeId>) {
    if self.linking {
      self.cancel_link();
      // Clicking empty space means "never mind".
      if let Some(target) = node {
        self.add_requirement(target);
      }
      return;
    }
    self.select(node);
  }

  /// Candidate requirement targets matching [`Self::link_filter`], capped at
  /// [`LINK_PICKER_MAX`]. Returns the rows and the total number of matches, so
  /// the panel can say how many it is not showing.
  pub fn candidate_requirements(&self) -> (Vec<(NodeId, String)>, usize) {
    let Some(id) = self.selected else {
      return (Vec::new(), 0);
    };
    let store = self.lock();
    let graph = store.graph();
    let existing: std::collections::HashSet<NodeId> =
      graph.requirements_of(id).map(|e| e.to).collect();
    let needle = self.link_filter.trim().to_lowercase();
    let mut v: Vec<(NodeId, String)> = graph
      .nodes()
      .filter(|n| n.id != id && !existing.contains(&n.id))
      .filter(|n| needle.is_empty() || n.name.to_lowercase().contains(&needle))
      .map(|n| (n.id, n.name.clone()))
      .collect();
    v.sort_by(|a, b| a.1.cmp(&b.1));
    let total = v.len();
    v.truncate(LINK_PICKER_MAX);
    (v, total)
  }

  /// Name and claim count of the active quest lens, or `None` in the global
  /// view. One lock for both, per the module note.
  pub fn active_quest_summary(&self) -> Option<(String, usize)> {
    let id = self.active_quest?;
    let store = self.lock();
    let quest = store.graph().quest(id)?;
    Some((quest.name.clone(), quest.claims.len()))
  }

  /// Whether the quest switcher is expanded.
  pub fn picker_open(&self) -> bool { self.picker_open }

  /// Expand or collapse the quest switcher.
  pub fn toggle_picker(&mut self) { self.picker_open = !self.picker_open; }

  /// Add a dependency requirement from the selected node to `target`.
  pub fn add_requirement(&mut self, target: NodeId) {
    let Some(id) = self.selected else { return };
    self.add_edge(id, target, EdgeKind::Dependency);
  }

  /// All quests as `(id, name, is_active)`, sorted by name.
  pub fn quest_list(&self) -> Vec<(QuestId, String, bool)> {
    let store = self.lock();
    let mut v: Vec<_> = store
      .graph()
      .quests()
      .map(|q| (q.id, q.name.clone(), self.active_quest == Some(q.id)))
      .collect();
    v.sort_by(|a, b| a.1.cmp(&b.1));
    v
  }

  /// The actionable frontier for the active quest, or all Ready nodes in the
  /// global view (PLAN §2 actionable query). Capped at [`ACTIONABLE_MAX`];
  /// the second value is the true total.
  pub fn actionable_list(&self) -> (Vec<(NodeId, String)>, usize) {
    let store = self.lock();
    let graph = store.graph();
    let derived = Derived::compute(graph);
    let ids: Vec<NodeId> = match self.active_quest {
      Some(q) => base::actionable(graph, &derived, q),
      None => {
        let mut v: Vec<NodeId> =
          derived.ready_nodes().iter().copied().collect();
        v.sort_unstable();
        v
      }
    };
    let mut v: Vec<(NodeId, String)> = ids
      .into_iter()
      .filter_map(|id| graph.node(id).map(|n| (id, n.name.clone())))
      .collect();
    let total = v.len();
    v.truncate(ACTIONABLE_MAX);
    (v, total)
  }

  /// Whether an undo is available.
  pub fn can_undo(&self) -> bool { self.lock().can_undo() }

  /// Whether a redo is available.
  pub fn can_redo(&self) -> bool { self.lock().can_redo() }

  // --- commands ---------------------------------------------------------

  fn commit(&mut self, events: Vec<Event>) {
    if let Err(e) = self.lock().commit(events) {
      // A local single-user tool: surface to the log and keep running rather
      // than crash mid-edit.
      eprintln!("commit failed: {e}");
    }
  }

  /// Select (or clear) the current node; resets the rename draft and disarms
  /// any pending requirement link.
  pub fn select(&mut self, node: Option<NodeId>) {
    self.cancel_link();
    self.selected = node;
    self.name_draft = {
      let store = self.lock();
      node
        .and_then(|id| store.graph().node(id))
        .map(|n| n.name.clone())
        .unwrap_or_default()
    };
  }

  /// Add a new task and select it.
  pub fn add_task(&mut self) {
    let id = NodeId::new();
    let hint = self.lock().graph().node_count() as f64;
    self.commit(vec![Event::NodeAdded {
      node:       id,
      kind:       NodeKind::task(),
      name:       "New task".into(),
      order_hint: hint,
    }]);
    self.select(Some(id));
  }

  /// Add a new condition and select it.
  pub fn add_condition(&mut self) {
    let id = NodeId::new();
    let hint = self.lock().graph().node_count() as f64;
    self.commit(vec![Event::NodeAdded {
      node:       id,
      kind:       NodeKind::condition(),
      name:       "New condition".into(),
      order_hint: hint,
    }]);
    self.select(Some(id));
  }

  /// Toggle the selected node's completion / satisfaction bit.
  pub fn toggle_selected(&mut self) {
    let Some(id) = self.selected else { return };
    let event = {
      let store = self.lock();
      let Some(node) = store.graph().node(id) else {
        return;
      };
      match &node.kind {
        NodeKind::Task { completed } => Event::TaskCompleted {
          node:      id,
          completed: !completed,
        },
        NodeKind::Condition { satisfied, .. } => Event::ConditionSet {
          node:      id,
          satisfied: !satisfied,
        },
      }
    };
    self.commit(vec![event]);
  }

  /// Commit `name` as the selected node's name. Called from the name field's
  /// Enter handler, so there is no separate confirm button to forget.
  pub fn rename_selected_to(&mut self, name: String) {
    let Some(id) = self.selected else { return };
    let name = name.trim().to_string();
    if name.is_empty() {
      return;
    }
    // Enter on an unchanged field should not push an undo entry.
    let unchanged =
      self.lock().graph().node(id).is_some_and(|n| n.name == name);
    if unchanged {
      return;
    }
    self.name_draft = name.clone();
    self.commit(vec![Event::NodeRenamed { node: id, name }]);
  }

  /// Delete the selected node (its incident edges and claims cascade; a
  /// single `NodeRemoved` event carries a complete inverse for undo).
  pub fn delete_selected(&mut self) {
    let Some(id) = self.selected else { return };
    self.commit(vec![Event::NodeRemoved { node: id }]);
    self.select(None);
  }

  /// Remove an incident edge, from either the requirements or the dependents
  /// list. `EdgeRemoved` captures the edge's endpoints for its inverse, so
  /// this undoes cleanly.
  pub fn remove_edge(&mut self, edge: EdgeId) {
    self.commit(vec![Event::EdgeRemoved { edge }]);
  }

  /// Add an edge `from -> to` (from requires to).
  pub fn add_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) {
    if from == to {
      return;
    }
    self.commit(vec![Event::EdgeAdded {
      edge: base::EdgeId::new(),
      kind,
      from,
      to,
    }]);
  }

  /// Create a new quest and make it the active lens.
  pub fn new_quest(&mut self) {
    let id = QuestId::new();
    self.commit(vec![Event::QuestCreated {
      quest: id,
      name:  "New quest".into(),
    }]);
    self.set_active_quest(Some(id));
  }

  /// Switch the active quest lens (or clear it for the global view), and
  /// collapse the switcher now that the choice is made.
  pub fn set_active_quest(&mut self, quest: Option<QuestId>) {
    self.active_quest = quest;
    self.picker_open = false;
  }

  /// Claim the selected node for the active quest.
  pub fn claim_selected(&mut self) {
    let (Some(q), Some(n)) = (self.active_quest, self.selected) else {
      return;
    };
    self.commit(vec![Event::QuestClaimed { quest: q, node: n }]);
  }

  /// Release the selected node's claim from the active quest.
  pub fn unclaim_selected(&mut self) {
    let (Some(q), Some(n)) = (self.active_quest, self.selected) else {
      return;
    };
    self.commit(vec![Event::QuestUnclaimed { quest: q, node: n }]);
  }

  /// Whether the selected node is claimed by the active quest.
  pub fn selected_is_claimed(&self) -> bool {
    let (Some(q), Some(n)) = (self.active_quest, self.selected) else {
      return false;
    };
    self
      .lock()
      .graph()
      .quest(q)
      .is_some_and(|q| q.claims.contains(&n))
  }

  /// Undo the last committed group.
  pub fn undo(&mut self) {
    if let Err(e) = self.lock().undo() {
      eprintln!("undo failed: {e}");
    }
    self.clamp_selection();
  }

  /// Redo the last undone group.
  pub fn redo(&mut self) {
    if let Err(e) = self.lock().redo() {
      eprintln!("redo failed: {e}");
    }
    self.clamp_selection();
  }

  /// Drop a selection that no longer resolves (e.g. after undoing an add).
  ///
  /// Re-selecting even when the node survives is deliberate: it refreshes
  /// `name_draft`, which an undone rename would otherwise leave showing the
  /// text that was just reverted away.
  fn clamp_selection(&mut self) {
    let Some(id) = self.selected else { return };
    let missing = self.lock().graph().node(id).is_none();
    self.select(if missing { None } else { Some(id) });
  }

  // --- demo seed --------------------------------------------------------

  /// Seed a small illustrative graph so a fresh database is not empty.
  fn seed_demo(&mut self) {
    let ship = NodeId::new();
    let backend = NodeId::new();
    let frontend = NodeId::new();
    let schema = NodeId::new();
    let signoff = NodeId::new();
    let quest = QuestId::new();

    let e = |from, to, kind| Event::EdgeAdded {
      edge: base::EdgeId::new(),
      kind,
      from,
      to,
    };
    let task = |id, name: &str, done| Event::NodeAdded {
      node:       id,
      kind:       NodeKind::Task { completed: done },
      name:       name.into(),
      order_hint: 0.0,
    };

    self.commit(vec![
      task(ship, "Ship v1", false),
      task(backend, "Build backend", false),
      task(frontend, "Build frontend", false),
      task(schema, "Design schema", true),
      Event::NodeAdded {
        node:       signoff,
        kind:       NodeKind::condition(),
        name:       "Design signed off".into(),
        order_hint: 0.0,
      },
      e(ship, backend, EdgeKind::Dependency),
      e(ship, frontend, EdgeKind::Dependency),
      e(backend, schema, EdgeKind::Dependency),
      e(frontend, signoff, EdgeKind::Dependency),
      Event::QuestCreated {
        quest,
        name: "v1 Launch".into(),
      },
      Event::QuestClaimed { quest, node: ship },
    ]);
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn scene_reflects_seeded_graph() {
    let store = Store::open_in_memory().unwrap();
    let state = AppState::new(store);
    let scene = state.scene();
    // 4 tasks + 1 condition were seeded.
    assert_eq!(scene.nodes.len(), 5);
    assert_eq!(scene.edges.len(), 4);
    // "Design schema" is done, so "Build backend" should be Ready.
    assert!(
      scene
        .nodes
        .iter()
        .any(|n| n.label == "Build backend" && n.state == NodeState::Ready)
    );
  }

  /// Id of the seeded node with the given name.
  fn node_named(state: &AppState, name: &str) -> NodeId {
    let store = state.lock();
    let id = store
      .graph()
      .nodes()
      .find(|n| n.name == name)
      .unwrap_or_else(|| panic!("no seeded node named {name}"))
      .id;
    id
  }

  #[test]
  fn selection_shows_both_edge_directions() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    let backend = node_named(&state, "Build backend");
    state.select(Some(backend));

    let info = state.selected_info().unwrap();
    // "Build backend" requires "Design schema", which is seeded completed.
    assert_eq!(info.requirements.len(), 1);
    assert_eq!(info.requirements[0].name, "Design schema");
    assert!(info.requirements[0].satisfied);
    // ...and "Ship v1" requires it, via the seeded subtask edge.
    assert_eq!(info.dependents.len(), 1);
    assert_eq!(info.dependents[0].name, "Ship v1");
    assert!(!info.dependents[0].satisfied);
  }

  #[test]
  fn removing_an_edge_undoes_cleanly() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    let backend = node_named(&state, "Build backend");
    state.select(Some(backend));

    let edge = state.selected_info().unwrap().requirements[0].edge;
    state.remove_edge(edge);

    let info = state.selected_info().unwrap();
    assert!(info.requirements.is_empty());
    // The opposite direction is untouched.
    assert_eq!(info.dependents.len(), 1);

    state.undo();
    let info = state.selected_info().unwrap();
    assert_eq!(info.requirements.len(), 1);
    assert_eq!(info.requirements[0].name, "Design schema");
    // The inverse restores the original edge id, not a fresh one.
    assert_eq!(info.requirements[0].edge, edge);
  }

  #[test]
  fn canvas_click_links_while_armed_and_selects_otherwise() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    let backend = node_named(&state, "Build backend");
    let frontend = node_named(&state, "Build frontend");

    // Unarmed, a click just moves the selection.
    state.select(Some(backend));
    assert!(!state.is_linking());
    state.canvas_click(Some(frontend));
    assert_eq!(state.selected, Some(frontend));

    // Armed, it builds an edge and leaves the selection put, so several
    // requirements can be added in a row.
    state.select(Some(backend));
    state.begin_link();
    assert!(state.is_linking());
    state.canvas_click(Some(frontend));
    assert_eq!(state.selected, Some(backend));
    assert!(!state.is_linking());
    let names: Vec<String> = state
      .selected_info()
      .unwrap()
      .requirements
      .into_iter()
      .map(|r| r.name)
      .collect();
    assert!(names.contains(&"Build frontend".to_string()));

    // Armed, a click on empty space cancels without linking or deselecting.
    state.begin_link();
    state.canvas_click(None);
    assert!(!state.is_linking());
    assert_eq!(state.selected, Some(backend));
  }

  #[test]
  fn renaming_commits_on_enter_and_stays_in_step_with_undo() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    let backend = node_named(&state, "Build backend");
    state.select(Some(backend));
    assert_eq!(state.name_draft, "Build backend");

    // Blank input is rejected rather than committing an empty name.
    state.rename_selected_to("   ".into());
    assert_eq!(state.name_draft, "Build backend");

    state.rename_selected_to("  Renamed  ".into());
    assert_eq!(state.name_draft, "Renamed");

    // Enter on unchanged text must not push a second undo entry — otherwise
    // this undo would land on "Renamed" instead of the original.
    state.rename_selected_to("Renamed".into());
    state.undo();
    assert_eq!(state.name_draft, "Build backend");
  }

  #[test]
  fn requirement_picker_is_bounded_and_filterable() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    let backend = node_named(&state, "Build backend");
    for _ in 0..20 {
      state.add_task();
    }
    state.select(Some(backend));

    // However big the graph gets, the panel shows a fixed number of rows.
    let (rows, total) = state.candidate_requirements();
    assert_eq!(rows.len(), LINK_PICKER_MAX);
    assert!(total > LINK_PICKER_MAX, "expected overflow, got {total}");

    state.link_filter = "signed".into();
    let (rows, total) = state.candidate_requirements();
    assert_eq!(total, 1);
    assert_eq!(rows[0].1, "Design signed off");
  }

  #[test]
  fn panel_resize_measures_from_the_press_anchor() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    assert_eq!(state.panel_width(), PANEL_WIDTH);

    // Dragging left widens the panel.
    state.begin_panel_resize();
    state.resize_panel(-40.0);
    assert_eq!(state.panel_width(), PANEL_WIDTH + 40.0);
    // Still the same drag: the delta is total travel, not an increment.
    state.resize_panel(-60.0);
    assert_eq!(state.panel_width(), PANEL_WIDTH + 60.0);

    // Overshooting clamps, and coming back does not drift: because the
    // anchor is fixed, returning the pointer restores the original width.
    state.begin_panel_resize();
    state.resize_panel(-10_000.0);
    assert_eq!(state.panel_width(), PANEL_MAX);
    state.resize_panel(0.0);
    assert_eq!(state.panel_width(), PANEL_WIDTH + 60.0);

    state.begin_panel_resize();
    state.resize_panel(10_000.0);
    assert_eq!(state.panel_width(), PANEL_MIN);
  }

  #[test]
  fn actionable_tracks_the_ready_frontier() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    // Global view: schema is done, so backend is ready; signoff pending is
    // actionable; frontend/ship blocked.
    let names: Vec<String> = state
      .actionable_list()
      .0
      .into_iter()
      .map(|(_, n)| n)
      .collect();
    assert!(names.contains(&"Build backend".to_string()));
    assert!(!names.contains(&"Ship v1".to_string()));

    // Undo/redo round-trips the store.
    let before = state.scene().nodes.len();
    state.add_task();
    assert_eq!(state.scene().nodes.len(), before + 1);
    state.undo();
    assert_eq!(state.scene().nodes.len(), before);
  }
}
