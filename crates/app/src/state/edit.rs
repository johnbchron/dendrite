//! Commands that change the graph, and undo and redo.

use base::{EdgeId, EdgeKind, Event, NodeId, NodeKind};

use super::{AppState, LiveEdit, selection::Primary};
use crate::{focus::FieldKey, formula::describe};

impl AppState {
  /// Add a new task: see `add_node`.
  pub fn add_task(&mut self) {
    self.add_node(NodeKind::task(), "New task");
  }

  /// Add a new condition: see `add_node`.
  pub fn add_condition(&mut self) {
    self.add_node(NodeKind::condition(), "New condition");
  }

  /// Add a node, select it and bring it into view. With a node selected, the
  /// new one becomes its requirement, in the same undo step, so it lands
  /// connected (below the selection) rather than as a tree of its own.
  fn add_node(&mut self, kind: NodeKind, name: &str) {
    let id = NodeId::new();
    let hint = self.lock().graph().hint_after(self.selected);
    let mut events = vec![Event::NodeAdded {
      node: id,
      kind,
      name: name.into(),
      order_hint: hint,
    }];
    if let Some(parent) = self.selected {
      events.push(Event::EdgeAdded {
        edge: EdgeId::new(),
        kind: EdgeKind::Dependency,
        from: parent,
        to: id,
      });
    }
    self.commit(events);
    self.go_to(id);
    // Straight into naming it; the name is selected, so typing replaces it.
    self.focus_requests.request(FieldKey::Title);
  }

  /// The selection's primary action: toggle a node's completion or
  /// satisfaction bit, or for a formula condition, change the fact it
  /// reads (see [`Primary`]).
  ///
  /// A task that is not Ready cannot be completed (PLAN §5): it would claim
  /// work done whose requirements are not.
  pub fn toggle_selected(&mut self) {
    let Some(id) = self.selected else { return };
    let Some(primary) = self.selected_info().map(|info| info.primary) else {
      return;
    };
    match primary {
      _ if !primary.enabled() => return,
      Primary::Here { place, here } => {
        return self.set_place((!here).then_some(place));
      }
      Primary::Toggle { context, .. } => return self.toggle_context(context),
      Primary::SetBalance => {
        return self.focus_requests.request(FieldKey::Balance);
      }
      Primary::EditSchedule => {
        return self.focus_requests.request(FieldKey::Span);
      }
      Primary::SetFreeTime => return self.edit_free_time(),
      _ => {}
    }
    let event = {
      let store = self.lock();
      let Some(node) = store.graph().node(id) else {
        return;
      };
      match &node.kind {
        NodeKind::Task { completed } => Event::TaskCompleted {
          node: id,
          completed: !completed,
        },
        NodeKind::Condition { satisfied, .. } => Event::ConditionSet {
          node: id,
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
    self.commit_live(
      LiveEdit::NodeName(id),
      vec![Event::NodeRenamed { node: id, name }],
    );
  }

  /// Enter in the name field: close the live edit, so further typing is a
  /// new undo step, and tidy the draft to the name the graph holds. A
  /// manual condition whose name clearly reads as a formula ("after oct 1",
  /// "at home") becomes that formula condition.
  pub fn finish_rename_selected(&mut self) {
    self.live_edit = None;
    if let Some(offer) = self.title_reading() {
      return self.make_automatic(offer);
    }
    let Some(id) = self.selected else { return };
    let name = self.lock().graph().node(id).map(|n| n.name.clone());
    if let Some(name) = name {
      self.name_draft = name;
    }
  }

  /// Delete the selected node (its incident edges and claims cascade; a
  /// single `NodeRemoved` event carries a complete inverse for undo).
  ///
  /// There is no confirmation: a toast offers Undo instead.
  pub fn delete_selected(&mut self) {
    let Some(id) = self.selected else { return };
    let name = {
      let store = self.lock();
      let graph = store.graph();
      graph
        .node(id)
        .map(|n| describe::node_name(graph, n, self.today()))
    };
    self.commit(vec![Event::NodeRemoved { node: id }]);
    self.select(None);
    let revision = self.lock().revision();
    self
      .toasts
      .show(format!("Deleted {}", name.unwrap_or_default()), revision);
  }

  /// Remove an incident edge, from either the requirements or the dependents
  /// list. `EdgeRemoved` captures the edge's endpoints for its inverse, so
  /// this undoes cleanly.
  pub fn remove_edge(&mut self, edge: EdgeId) {
    self.commit(vec![Event::EdgeRemoved { edge }]);
  }

  /// Add an edge `from -> to` (from requires to).
  ///
  /// A no-op if `from` already requires `to`: a second edge would add
  /// nothing but a duplicate row in the panel and a second arrow on the
  /// canvas.
  pub fn add_edge(&mut self, from: NodeId, to: NodeId, kind: EdgeKind) {
    if from == to {
      return;
    }
    let exists = self
      .lock()
      .graph()
      .requirements_of(from)
      .any(|e| e.to == to);
    if exists {
      return;
    }
    self.commit(vec![Event::EdgeAdded {
      edge: EdgeId::new(),
      kind,
      from,
      to,
    }]);
  }

  /// Whether an undo is available.
  pub fn can_undo(&self) -> bool {
    self.lock().can_undo()
  }

  /// Whether a redo is available.
  pub fn can_redo(&self) -> bool {
    self.lock().can_redo()
  }

  /// What undo would reverse ("rename"), if anything.
  pub fn undo_label(&self) -> Option<&'static str> {
    self.lock().undo_label()
  }

  /// What redo would re-apply, if anything.
  pub fn redo_label(&self) -> Option<&'static str> {
    self.lock().redo_label()
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
