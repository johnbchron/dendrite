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

use crate::{
  canvas::{CanvasScene, RenderEdge, RenderNode},
  theme::{self, Theme},
};

/// The whole application's state.
pub struct AppState {
  store:            Mutex<Store>,
  /// Currently selected node, if any.
  pub selected:     Option<NodeId>,
  /// Active quest lens; `None` means the global "all nodes" view (PLAN §5).
  pub active_quest: Option<QuestId>,
  /// Editable name buffer for the selected node.
  pub name_draft:   String,
  /// Editable name buffer for the active quest, shown in the switcher.
  pub quest_draft:  String,
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
  /// The palette every painted surface reads its colours from.
  theme:            &'static Theme,
  /// Whether the palette picker is expanded under the toolbar.
  palette_open:     bool,
  /// The field whose keystrokes are currently being committed, if the last
  /// commit came from one. The next keystroke in the same field amends that
  /// undo group rather than opening a new one.
  live_edit:        Option<LiveEdit>,
}

/// A text field that commits as it is typed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LiveEdit {
  /// The inspector's name field, for this node.
  NodeName(NodeId),
  /// The quest switcher's rename field, for this quest.
  QuestName(QuestId),
}

/// `meta` key the chosen palette is stored under.
const THEME_KEY: &str = "palette";

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
    let mut state = Self {
      store:            Mutex::new(store),
      selected:         None,
      active_quest:     None,
      name_draft:       String::new(),
      quest_draft:      String::new(),
      recenter_epoch:   0,
      panel_width:      PANEL_WIDTH,
      panel_width_base: PANEL_WIDTH,
      linking:          false,
      link_filter:      String::new(),
      picker_open:      false,
      theme:            theme::DEFAULT,
      palette_open:     false,
      live_edit:        None,
    };
    // A palette recorded by an older version that no longer ships falls back
    // to the default rather than blocking startup.
    let stored = match state.lock().setting(THEME_KEY) {
      Ok(id) => id,
      Err(e) => {
        eprintln!("reading the stored palette failed: {e}");
        None
      }
    };
    state.theme = stored
      .and_then(|id| theme::by_id(&id))
      .unwrap_or(theme::DEFAULT);
    state
  }

  /// The active palette. Every colour in the canvas and the panel comes from
  /// here.
  pub fn theme(&self) -> &'static Theme { self.theme }

  /// Switch palettes and persist the choice. A failed write is reported and
  /// the palette still changes for this session.
  pub fn set_theme(&mut self, id: &str) {
    let Some(next) = theme::by_id(id) else { return };
    self.theme = next;
    self.palette_open = false;
    if let Err(e) = self.lock().set_setting(THEME_KEY, next.id) {
      eprintln!("saving the palette failed: {e}");
    }
  }

  /// Every palette as `(id, name, is_active)`, for the picker.
  pub fn theme_list(&self) -> Vec<(&'static str, &'static str, bool)> {
    theme::ALL
      .iter()
      .map(|t| (t.id, t.name, t.id == self.theme.id))
      .collect()
  }

  /// Whether the palette picker is expanded.
  pub fn palette_open(&self) -> bool { self.palette_open }

  /// Expand or collapse the palette picker.
  pub fn toggle_palette(&mut self) { self.palette_open = !self.palette_open; }

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
      nodes.push(RenderNode {
        id:       node.id,
        label:    node.name.clone(),
        kind:     node.kind.clone(),
        state:    derived.state(node.id).unwrap_or(NodeState::Blocked),
        selected: self.selected == Some(node.id),
        dimmed:   scoped && !claimed.contains(&node.id),
      });
    }

    let mut edges = Vec::new();
    for edge in graph.edges() {
      if !visible.contains(&edge.from) || !visible.contains(&edge.to) {
        continue;
      }
      edges.push(RenderEdge {
        from:     edge.from,
        to:       edge.to,
        kind:     edge.kind,
        reversed: lay.is_reversed(edge.id),
      });
    }

    // Out-of-scope nodes give up their slots, so a lens shows its nodes
    // packed together rather than scattered among gaps.
    let arrangement = lay.arrangement.retain(|n| visible.contains(&n));
    CanvasScene {
      nodes,
      edges,
      arrangement,
    }
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
    self.live_edit = None;
    if let Err(e) = self.lock().commit(events) {
      // A local single-user tool: surface to the log and keep running rather
      // than crash mid-edit.
      eprintln!("commit failed: {e}");
    }
  }

  /// Commit a keystroke's worth of change from the field `edit`. Successive
  /// keystrokes in one field fold into a single undo group, so typing a name
  /// costs one undo step, not one per character; any other commit, a change
  /// of selection, or Enter closes the group.
  fn commit_live(&mut self, edit: LiveEdit, events: Vec<Event>) {
    let amend = self.live_edit == Some(edit);
    let result = if amend {
      self.lock().commit_amend(events)
    } else {
      self.lock().commit(events)
    };
    if let Err(e) = result {
      eprintln!("commit failed: {e}");
    }
    self.live_edit = Some(edit);
  }

  /// Select (or clear) the current node; resets the rename draft and disarms
  /// any pending requirement link.
  pub fn select(&mut self, node: Option<NodeId>) {
    self.cancel_link();
    self.live_edit = None;
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

  /// The name field changed: keep the draft exactly as typed and commit its
  /// trimmed form as the selected node's name, so the canvas follows along
  /// and there is no confirm step to forget.
  ///
  /// Blank text is held in the draft but not committed (the field is
  /// mid-retype), and text that trims to the current name commits nothing.
  pub fn rename_selected_to(&mut self, text: String) {
    self.name_draft = text;
    let Some(id) = self.selected else { return };
    let name = self.name_draft.trim().to_string();
    if name.is_empty() {
      return;
    }
    let unchanged =
      self.lock().graph().node(id).is_some_and(|n| n.name == name);
    if unchanged {
      return;
    }
    self.commit_live(LiveEdit::NodeName(id), vec![Event::NodeRenamed {
      node: id,
      name,
    }]);
  }

  /// Enter in the name field: close the live edit, so further typing is a
  /// new undo step, and tidy the draft to the name the graph holds.
  pub fn finish_rename_selected(&mut self) {
    self.live_edit = None;
    let Some(id) = self.selected else { return };
    let name = self.lock().graph().node(id).map(|n| n.name.clone());
    if let Some(name) = name {
      self.name_draft = name;
    }
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
  ///
  /// The switcher stays open afterwards: a new quest is called "New quest"
  /// until it is renamed, and the rename field lives in the switcher.
  pub fn new_quest(&mut self) {
    let id = QuestId::new();
    self.commit(vec![Event::QuestCreated {
      quest: id,
      name:  "New quest".into(),
    }]);
    self.set_active_quest(Some(id));
    self.picker_open = true;
  }

  /// Switch the active quest lens (or clear it for the global view), and
  /// collapse the switcher now that the choice is made.
  pub fn set_active_quest(&mut self, quest: Option<QuestId>) {
    self.live_edit = None;
    self.active_quest = quest;
    self.picker_open = false;
    self.sync_quest_draft();
  }

  /// Refresh the quest rename buffer from the graph. Called whenever the
  /// active quest changes and after undo/redo, so the field never shows a
  /// name the graph no longer holds.
  fn sync_quest_draft(&mut self) {
    self.quest_draft = {
      let store = self.lock();
      self
        .active_quest
        .and_then(|id| store.graph().quest(id))
        .map(|q| q.name.clone())
        .unwrap_or_default()
    };
  }

  /// The switcher's rename field changed. Mirrors
  /// [`AppState::rename_selected_to`]: the draft follows every keystroke, the
  /// trimmed text is committed live, and blank or unchanged text commits
  /// nothing.
  pub fn rename_active_quest_to(&mut self, text: String) {
    self.quest_draft = text;
    let Some(id) = self.active_quest else { return };
    let name = self.quest_draft.trim().to_string();
    if name.is_empty() {
      return;
    }
    let unchanged = self
      .lock()
      .graph()
      .quest(id)
      .is_some_and(|q| q.name == name);
    if unchanged {
      return;
    }
    self.commit_live(LiveEdit::QuestName(id), vec![Event::QuestRenamed {
      quest: id,
      name,
    }]);
  }

  /// Enter in the quest rename field: the counterpart of
  /// [`AppState::finish_rename_selected`].
  pub fn finish_rename_quest(&mut self) {
    self.live_edit = None;
    self.sync_quest_draft();
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
    self.live_edit = None;
    if let Err(e) = self.lock().undo() {
      eprintln!("undo failed: {e}");
    }
    self.clamp_selection();
    self.clamp_active_quest();
  }

  /// Redo the last undone group.
  pub fn redo(&mut self) {
    self.live_edit = None;
    if let Err(e) = self.lock().redo() {
      eprintln!("redo failed: {e}");
    }
    self.clamp_selection();
    self.clamp_active_quest();
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

  /// The same for the quest lens: drop an active quest that no longer
  /// resolves, and otherwise refresh `quest_draft` against the graph.
  fn clamp_active_quest(&mut self) {
    let Some(id) = self.active_quest else { return };
    if self.lock().graph().quest(id).is_none() {
      self.active_quest = None;
    }
    self.sync_quest_draft();
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A store holding the small demo graph the app used to seed on first run:
  ///
  /// - "Ship v1" requires "Build backend" and "Build frontend";
  /// - "Build backend" requires "Design schema" (completed);
  /// - "Build frontend" requires the condition "Design signed off";
  /// - the quest "v1 Launch" claims "Ship v1".
  fn demo_store() -> Store {
    let mut store = Store::open_in_memory().unwrap();
    let ship = NodeId::new();
    let backend = NodeId::new();
    let frontend = NodeId::new();
    let schema = NodeId::new();
    let signoff = NodeId::new();
    let quest = QuestId::new();

    let edge = |from, to| Event::EdgeAdded {
      edge: EdgeId::new(),
      kind: EdgeKind::Dependency,
      from,
      to,
    };
    let node = |node, name: &str, kind| Event::NodeAdded {
      node,
      kind,
      name: name.into(),
      order_hint: 0.0,
    };
    store
      .commit(vec![
        node(ship, "Ship v1", NodeKind::task()),
        node(backend, "Build backend", NodeKind::task()),
        node(frontend, "Build frontend", NodeKind::task()),
        node(schema, "Design schema", NodeKind::Task { completed: true }),
        node(signoff, "Design signed off", NodeKind::condition()),
        edge(ship, backend),
        edge(ship, frontend),
        edge(backend, schema),
        edge(frontend, signoff),
        Event::QuestCreated {
          quest,
          name: "v1 Launch".into(),
        },
        Event::QuestClaimed { quest, node: ship },
      ])
      .unwrap();
    store
  }

  #[test]
  fn scene_reflects_seeded_graph() {
    let state = AppState::new(demo_store());
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

  #[test]
  fn renaming_a_quest_commits_trims_and_undoes() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    state.new_quest();
    let id = state.active_quest.expect("new quest became the lens");
    assert_eq!(
      state.quest_draft, "New quest",
      "draft seeded from the graph"
    );

    // Blank input is held in the draft but not committed.
    state.rename_active_quest_to("   ".into());
    assert_eq!(state.active_quest_summary().unwrap().0, "New quest");

    // Every keystroke commits the trimmed text; the draft keeps what was
    // typed until Enter tidies it.
    for text in ["  S", "  Sh", "  Ship v1  "] {
      state.rename_active_quest_to(text.into());
    }
    assert_eq!(state.active_quest_summary().unwrap().0, "Ship v1");
    assert_eq!(state.quest_draft, "  Ship v1  ");
    state.finish_rename_quest();
    assert_eq!(state.quest_draft, "Ship v1");

    // One undo reverts the whole rename and pulls the draft back with it.
    state.undo();
    assert_eq!(state.active_quest_summary().unwrap().0, "New quest");
    assert_eq!(state.quest_draft, "New quest");

    // Undoing the creation drops the lens rather than leaving it dangling.
    state.undo();
    assert_eq!(state.active_quest, None);
    assert!(state.quest_draft.is_empty());
    assert!(state.lock().graph().quest(id).is_none());
  }

  #[test]
  fn renaming_a_quest_in_the_global_view_is_a_no_op() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    state.new_quest();
    state.set_active_quest(None);
    state.rename_active_quest_to("Ship v1".into());
    assert!(
      state
        .quest_list()
        .iter()
        .all(|(_, name, _)| name == "New quest"),
      "no quest was renamed from the global view"
    );
  }

  #[test]
  fn palette_choice_defaults_persists_and_survives_a_bad_id() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    assert_eq!(state.theme().id, theme::DEFAULT.id, "default when unset");

    state.set_theme("umber");
    assert_eq!(state.theme().id, "umber");
    assert_eq!(
      state.lock().setting("palette").unwrap().as_deref(),
      Some("umber"),
      "the choice was written to the meta table"
    );

    // Exactly one palette reads as active in the picker.
    let active: Vec<&str> = state
      .theme_list()
      .iter()
      .filter(|(_, _, is_active)| *is_active)
      .map(|(id, ..)| *id)
      .collect();
    assert_eq!(active, vec!["umber"]);

    // An id this build does not ship is ignored rather than blanking the UI.
    state.set_theme("chartreuse");
    assert_eq!(state.theme().id, "umber");
  }

  #[test]
  fn a_stored_palette_is_restored_on_open() {
    let store = Store::open_in_memory().unwrap();
    store.set_setting("palette", "meridian").unwrap();
    let state = AppState::new(store);
    assert_eq!(state.theme().id, "meridian");
  }

  #[test]
  fn an_unknown_stored_palette_falls_back_to_the_default() {
    let store = Store::open_in_memory().unwrap();
    store.set_setting("palette", "chartreuse").unwrap();
    let state = AppState::new(store);
    assert_eq!(state.theme().id, theme::DEFAULT.id);
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
    let mut state = AppState::new(demo_store());
    let backend = node_named(&state, "Build backend");
    state.select(Some(backend));

    let info = state.selected_info().unwrap();
    // "Build backend" requires "Design schema", which is seeded completed.
    assert_eq!(info.requirements.len(), 1);
    assert_eq!(info.requirements[0].name, "Design schema");
    assert!(info.requirements[0].satisfied);
    // ...and "Ship v1" requires it.
    assert_eq!(info.dependents.len(), 1);
    assert_eq!(info.dependents[0].name, "Ship v1");
    assert!(!info.dependents[0].satisfied);
  }

  #[test]
  fn removing_an_edge_undoes_cleanly() {
    let mut state = AppState::new(demo_store());
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
    let mut state = AppState::new(demo_store());
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
  fn renaming_commits_per_keystroke_as_one_undo_step() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    state.add_task();
    let id = state.selected.expect("a new task is selected");
    let name = |s: &AppState| s.lock().graph().node(id).unwrap().name.clone();
    assert_eq!(state.name_draft, "New task");

    // Clearing the field to retype is held in the draft, not committed.
    state.rename_selected_to("".into());
    assert_eq!(state.name_draft, "");
    assert_eq!(name(&state), "New task");

    // Each keystroke lands in the graph, trimmed, with the draft left as
    // typed so the cursor is not disturbed.
    for text in ["R", "Re", "Ren ", "Renamed "] {
      state.rename_selected_to(text.into());
    }
    assert_eq!(name(&state), "Renamed");
    assert_eq!(state.name_draft, "Renamed ");
    state.finish_rename_selected();
    assert_eq!(state.name_draft, "Renamed");

    // Typing after Enter is a second undo step.
    state.rename_selected_to("Renamed again".into());
    state.undo();
    assert_eq!(name(&state), "Renamed");
    assert_eq!(state.name_draft, "Renamed");

    // The whole first edit undoes in one step, leaving the add intact.
    state.undo();
    assert_eq!(name(&state), "New task");
    assert_eq!(state.name_draft, "New task");
  }

  #[test]
  fn selecting_another_node_closes_the_live_edit() {
    let store = Store::open_in_memory().unwrap();
    let mut state = AppState::new(store);
    state.add_task();
    let a = state.selected.unwrap();
    state.add_task();
    let b = state.selected.unwrap();

    state.select(Some(a));
    state.rename_selected_to("A".into());
    state.select(Some(b));
    state.rename_selected_to("B".into());

    // Two fields, two undo steps: undoing B's rename leaves A's alone.
    state.undo();
    let graph_name = |id| state.lock().graph().node(id).unwrap().name.clone();
    assert_eq!(graph_name(a), "A");
    assert_eq!(graph_name(b), "New task");
  }

  #[test]
  fn requirement_picker_is_bounded_and_filterable() {
    let mut state = AppState::new(demo_store());
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
    let mut state = AppState::new(demo_store());
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
