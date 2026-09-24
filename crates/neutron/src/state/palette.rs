//! The command palette's state: what it lists for the typed query, and what
//! choosing a row does.
//!
//! One list mixes three kinds of row: nodes (choose to go to one), commands
//! (verb phrases, with their keys), and quests (switch the lens). With an
//! empty query it shows recently selected nodes, then the commands; typing
//! ranks everything by [`query::score`].

use base::{NodeId, NodeState, QuestId};

use super::AppState;
use crate::{
  canvas::ZoomStep,
  focus::FieldKey,
  keymap::chord,
  query::{self, Query},
  theme,
};

/// Most rows the palette shows; past that, typing narrows.
pub const PALETTE_MAX: usize = 9;
/// How many recently selected nodes the empty palette offers.
const RECENT_MAX: usize = 5;

/// What choosing a palette row does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PaletteAct {
  /// Select a node and bring it into view.
  GoTo(NodeId),
  /// Switch the lens (`None`: all nodes).
  Quest(Option<QuestId>),
  /// Create a task (attached to the selection, if any).
  NewTask,
  /// Create a condition (attached to the selection, if any).
  NewCondition,
  /// Undo.
  Undo,
  /// Redo.
  Redo,
  /// Fit the graph.
  Fit,
  /// Step the zoom.
  Zoom(ZoomStep),
  /// Open or close the Now tray.
  ToggleNow,
  /// Open the quest switcher.
  OpenQuests,
  /// Switch palette (colour theme), by id.
  Theme(&'static str),
  /// The selection's primary action (complete, reopen, satisfy...).
  Primary,
  /// Arm link mode for the selection, in the requirement search.
  Require,
  /// Put the cursor in the selection's name.
  Rename,
  /// Delete the selection.
  Delete,
}

/// What kind of thing a row is, for its icon and its place in the ranking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum RowKind {
  /// A node.
  Node,
  /// A command.
  Command,
  /// A quest.
  Quest,
}

/// One row of the palette.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PaletteRow {
  /// What choosing it does.
  pub act:    PaletteAct,
  /// What it is.
  pub kind:   RowKind,
  /// What it says.
  pub label:  String,
  /// A trailing note: a command's key, the quests claiming a node.
  pub detail: Option<String>,
  /// A node's state, for its dot.
  pub state:  Option<NodeState>,
}

impl AppState {
  /// Whether the command palette is open.
  pub fn palette_open(&self) -> bool { self.palette_open }

  /// The palette's query, for its search box and highlight.
  pub fn palette_query(&self) -> &Query { &self.palette_query }

  /// The palette's search field changed.
  pub fn set_palette_text(&mut self, text: String) {
    self.palette_query.set_text(text);
  }

  /// Whether the palette was opened to search nodes only (with `/`).
  pub fn palette_nodes_only(&self) -> bool { self.palette_nodes_only }

  /// Open the palette with an empty query; `nodes_only` limits it to nodes.
  pub fn open_palette(&mut self, nodes_only: bool) {
    self.close_popovers();
    self.cancel_link();
    self.palette_query = Query::default();
    self.palette_nodes_only = nodes_only;
    self.palette_open = true;
    self.focus_requests.request(FieldKey::PaletteSearch);
  }

  /// Remember `node` as recently selected, for the empty palette.
  pub(super) fn remember(&mut self, node: NodeId) {
    self.recent.retain(|n| *n != node);
    self.recent.insert(0, node);
    self.recent.truncate(RECENT_MAX);
  }

  /// The palette's rows for its query: at most [`PALETTE_MAX`], best first,
  /// and how many matched in all.
  pub fn palette_rows(&self) -> (Vec<PaletteRow>, usize) {
    let text = self.palette_query.text.trim();
    let mut rows = self.node_rows(text.is_empty());
    if !self.palette_nodes_only {
      rows.extend(self.command_rows());
      rows.extend(self.quest_rows_for_palette());
    }
    if text.is_empty() {
      // Recent nodes, then the commands (quests are one keystroke away).
      rows.retain(|r| r.kind != RowKind::Quest);
      let total = rows.len();
      rows.truncate(PALETTE_MAX);
      return (rows, total);
    }
    let mut scored: Vec<(u32, RowKind, PaletteRow)> = rows
      .into_iter()
      .filter_map(|row| {
        query::score(text, &row.label).map(|sc| (sc, row.kind, row))
      })
      .collect();
    // Best score; on a tie, nodes before commands before quests, then by
    // label, so the order is stable while typing.
    scored.sort_by(|a, b| {
      a.0
        .cmp(&b.0)
        .then(a.1.cmp(&b.1))
        .then(a.2.label.cmp(&b.2.label))
    });
    let total = scored.len();
    let rows = scored
      .into_iter()
      .take(PALETTE_MAX)
      .map(|(_, _, row)| row)
      .collect();
    (rows, total)
  }

  /// Every node — or, for the empty query, the recently selected ones.
  fn node_rows(&self, recent_only: bool) -> Vec<PaletteRow> {
    let store = self.lock();
    let graph = store.graph();
    let cached = self.derivations(&store);
    let row = |id: NodeId| {
      let node = graph.node(id)?;
      let quests: Vec<String> = base::claiming_quests(graph, id)
        .into_iter()
        .filter_map(|q| graph.quest(q).map(|q| q.name.clone()))
        .collect();
      Some(PaletteRow {
        act:    PaletteAct::GoTo(id),
        kind:   RowKind::Node,
        label:  node.name.clone(),
        detail: (!quests.is_empty()).then(|| quests.join(", ")),
        state:  cached.derived.state(id),
      })
    };
    if recent_only {
      self.recent.iter().filter_map(|id| row(*id)).collect()
    } else {
      graph.nodes().filter_map(|n| row(n.id)).collect()
    }
  }

  /// The commands available now, as verb phrases with their keys.
  fn command_rows(&self) -> Vec<PaletteRow> {
    let command = |act, label: String, key: Option<String>| PaletteRow {
      act,
      kind: RowKind::Command,
      label,
      detail: key,
      state: None,
    };
    let key = |k: &str| Some(k.to_string());
    let mut rows = Vec::new();
    if let Some(info) = self.selected_info() {
      let name = self.name_draft.trim().to_string();
      if info.primary.enabled() {
        rows.push(command(
          PaletteAct::Primary,
          format!("{}: {name}", info.primary.label()),
          key("Space"),
        ));
      }
      rows.push(command(
        PaletteAct::Rename,
        format!("Rename {name}"),
        key("Enter"),
      ));
      rows.push(command(
        PaletteAct::Require,
        format!("Add a requirement to {name}"),
        key("R"),
      ));
      rows.push(command(
        PaletteAct::Delete,
        format!("Delete {name}"),
        key("Del"),
      ));
    }
    let attach = self.attach_point();
    let new = |what: &str| match &attach {
      Some(name) => format!("New {what} required by {name}"),
      None => format!("New {what}"),
    };
    rows.push(command(PaletteAct::NewTask, new("task"), key("N")));
    rows.push(command(
      PaletteAct::NewCondition,
      new("condition"),
      key("Shift+N"),
    ));
    if let Some(step) = self.undo_label() {
      rows.push(command(
        PaletteAct::Undo,
        format!("Undo {step}"),
        Some(chord("Z")),
      ));
    }
    if let Some(step) = self.redo_label() {
      rows.push(command(
        PaletteAct::Redo,
        format!("Redo {step}"),
        Some(chord("Shift+Z")),
      ));
    }
    rows.push(command(PaletteAct::Fit, "Fit the graph".into(), key("F")));
    rows.push(command(
      PaletteAct::Zoom(ZoomStep::In),
      "Zoom in".into(),
      Some(chord("=")),
    ));
    rows.push(command(
      PaletteAct::Zoom(ZoomStep::Out),
      "Zoom out".into(),
      Some(chord("-")),
    ));
    rows.push(command(
      PaletteAct::Zoom(ZoomStep::Reset),
      "Reset zoom to 100%".into(),
      Some(chord("0")),
    ));
    rows.push(command(
      PaletteAct::ToggleNow,
      if self.now_open {
        "Hide the Now tray"
      } else {
        "Show the Now tray"
      }
      .into(),
      key("A"),
    ));
    rows.push(command(
      PaletteAct::OpenQuests,
      "Switch quest".into(),
      key("Q"),
    ));
    for t in theme::ALL {
      if t.id != self.theme.id {
        rows.push(command(
          PaletteAct::Theme(t.id),
          format!("Switch palette to {}", t.name),
          None,
        ));
      }
    }
    rows
  }

  /// "Switch to <quest>" for every quest but the current lens, and back to
  /// all nodes when under one.
  fn quest_rows_for_palette(&self) -> Vec<PaletteRow> {
    let store = self.lock();
    let mut quests: Vec<_> = store
      .graph()
      .quests()
      .filter(|q| self.active_quest != Some(q.id))
      .map(|q| (q.name.clone(), q.id))
      .collect();
    quests.sort();
    let mut rows: Vec<PaletteRow> = quests
      .into_iter()
      .map(|(name, id)| PaletteRow {
        act:    PaletteAct::Quest(Some(id)),
        kind:   RowKind::Quest,
        label:  format!("Switch to {name}"),
        detail: Some("Quest".into()),
        state:  None,
      })
      .collect();
    if self.active_quest.is_some() {
      rows.push(PaletteRow {
        act:    PaletteAct::Quest(None),
        kind:   RowKind::Quest,
        label:  "Switch to all nodes".into(),
        detail: Some("Quest".into()),
        state:  None,
      });
    }
    rows
  }

  /// Choose the highlighted row (Enter).
  pub fn accept_palette(&mut self) {
    let (rows, _) = self.palette_rows();
    if let Some(row) = rows.get(self.palette_query.highlighted(rows.len())) {
      self.run_palette(row.act);
    }
  }

  /// Close the palette and do what a row says.
  pub fn run_palette(&mut self, act: PaletteAct) {
    self.palette_open = false;
    match act {
      PaletteAct::GoTo(node) => {
        // A node outside the lens is not drawn: leave the lens to show it.
        let hidden = self
          .active_quest
          .is_some_and(|q| !base::scope(self.lock().graph(), q).contains(node));
        if hidden {
          self.set_active_quest(None);
        }
        self.go_to(node);
      }
      PaletteAct::Quest(quest) => self.set_active_quest(quest),
      PaletteAct::NewTask => self.add_task(),
      PaletteAct::NewCondition => self.add_condition(),
      PaletteAct::Undo => self.undo(),
      PaletteAct::Redo => self.redo(),
      PaletteAct::Fit => self.recenter(),
      PaletteAct::Zoom(step) => self.zoom(step),
      PaletteAct::ToggleNow => self.toggle_now(),
      PaletteAct::OpenQuests => self.toggle_picker(),
      PaletteAct::Theme(id) => self.set_theme(id),
      PaletteAct::Primary => self.toggle_selected(),
      PaletteAct::Require => {
        self.begin_link();
        self.focus_requests.request(FieldKey::LinkSearch);
      }
      PaletteAct::Rename => self.focus_requests.request(FieldKey::Title),
      PaletteAct::Delete => self.delete_selected(),
    }
  }
}
