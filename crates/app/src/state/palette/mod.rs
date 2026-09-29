//! The command palette's state: what it lists for the typed query, and what
//! choosing a row does.
//!
//! One list mixes three kinds of row: nodes (choose to go to one), commands
//! (verb phrases, with their keys), and quests (switch the lens, rename or
//! delete one, or add the selection to one or take it out). With an empty query
//! it shows recently selected nodes, then the commands; typing ranks everything
//! by [`query::score`](crate::query::score).

mod rows;

use base::{ContextId, NodeId, NodeState, PlaceId, QuestId};

use super::{AppState, RefKey, RefKind, chrome::Popover};
use crate::{camera::ZoomStep, focus::FieldKey, query::Query};

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
  /// Switch to the completed lens.
  Completed,
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
  /// Open the place picker, in the Now tray.
  PickPlace,
  /// Open the quest switcher.
  OpenQuests,
  /// Remove unused formula conditions and referents.
  Prune,
  /// Open the library of places, contexts, resources and schedules.
  OpenLibrary,
  /// Declare where I am (`None`: nowhere I have named).
  SetPlace(Option<PlaceId>),
  /// Turn a context on or off.
  ToggleContext(ContextId),
  /// Put the cursor in a referent's name, in the library.
  RenameReferent(RefKey),
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
  /// Add the selection to a quest.
  Claim(QuestId),
  /// Take the selection out of a quest.
  Unclaim(QuestId),
  /// Start a quest with the selection in it.
  NewQuestWith,
  /// Put the cursor in a quest's name.
  RenameQuest(QuestId),
  /// Delete a quest.
  DeleteQuest(QuestId),
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
  /// A place, context, resource or schedule.
  Referent(RefKind),
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

impl PaletteRow {
  /// A command row: a verb phrase, and the key that runs it, if any.
  fn command(act: PaletteAct, label: String, key: Option<String>) -> Self {
    Self {
      act,
      kind: RowKind::Command,
      label,
      detail: key,
      state: None,
    }
  }

  /// A row that acts on a quest.
  fn quest(act: PaletteAct, label: String) -> Self {
    Self {
      act,
      kind: RowKind::Quest,
      label,
      detail: Some("Quest".into()),
      state: None,
    }
  }
}

/// Recently selected nodes, newest first, for the empty palette.
#[derive(Clone, Debug, Default)]
pub(super) struct Recent(Vec<NodeId>);

impl Recent {
  /// Remember `node` as the most recently selected.
  pub(super) fn remember(&mut self, node: NodeId) {
    self.0.retain(|n| *n != node);
    self.0.insert(0, node);
    self.0.truncate(RECENT_MAX);
  }

  /// The nodes, newest first.
  fn iter(&self) -> impl Iterator<Item = NodeId> + '_ { self.0.iter().copied() }
}

impl AppState {
  /// Whether the command palette is open.
  pub fn palette_open(&self) -> bool { self.popover == Some(Popover::Palette) }

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
    self.popover = Some(Popover::Palette);
    self.focus_requests.request(FieldKey::PaletteSearch);
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
    self.close(Popover::Palette);
    match act {
      PaletteAct::GoTo(node) => self.reveal(node),
      PaletteAct::Quest(quest) => self.set_active_quest(quest),
      PaletteAct::Completed => self.show_completed(),
      PaletteAct::NewTask => self.add_task(),
      PaletteAct::NewCondition => self.add_condition(),
      PaletteAct::Undo => self.undo(),
      PaletteAct::Redo => self.redo(),
      PaletteAct::Fit => self.recenter(),
      PaletteAct::Zoom(step) => self.zoom(step),
      PaletteAct::ToggleNow => self.toggle_now(),
      PaletteAct::PickPlace => self.toggle_place_picker(),
      PaletteAct::OpenQuests => self.toggle_picker(),
      PaletteAct::Prune => self.prune(),
      PaletteAct::OpenLibrary => self.toggle_library(),
      PaletteAct::SetPlace(place) => self.set_place(place),
      PaletteAct::ToggleContext(context) => self.toggle_context(context),
      PaletteAct::RenameReferent(key) => self.rename_referent(key),
      PaletteAct::Theme(id) => self.set_theme(id),
      PaletteAct::Primary => self.toggle_selected(),
      PaletteAct::Require => {
        self.begin_link();
        self.focus_requests.request(FieldKey::LinkSearch);
      }
      PaletteAct::Rename => self.focus_requests.request(FieldKey::Title),
      PaletteAct::Delete => self.delete_selected(),
      PaletteAct::Claim(quest) => self.claim_selected(quest),
      PaletteAct::Unclaim(quest) => self.unclaim_selected(quest),
      PaletteAct::NewQuestWith => self.new_quest_with_selected(),
      PaletteAct::RenameQuest(quest) => self.rename_quest(quest),
      PaletteAct::DeleteQuest(quest) => self.delete_quest(quest),
    }
  }
}
