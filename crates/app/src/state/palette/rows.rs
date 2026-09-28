//! What the palette lists: nodes, commands, and quest rows, ranked by the
//! query.

use base::NodeId;

use super::{PALETTE_MAX, PaletteAct, PaletteRow, RowKind};
use crate::{
  camera::ZoomStep, keymap::chord, query, state::AppState, theme::Theme,
};

impl AppState {
  /// The palette's rows for its query: at most `PALETTE_MAX`, best first,
  /// and how many matched in all.
  pub fn palette_rows(&self) -> (Vec<PaletteRow>, usize) {
    let text = self.palette_query.text.trim();
    let mut rows = self.node_rows(text.is_empty());
    if !self.palette_nodes_only {
      rows.extend(self.command_rows());
      rows.extend(self.quest_rows_for_palette());
      rows.extend(self.membership_rows());
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
      self.recent.iter().filter_map(row).collect()
    } else {
      graph.nodes().filter_map(|n| row(n.id)).collect()
    }
  }

  /// The commands available now, as verb phrases with their keys.
  fn command_rows(&self) -> Vec<PaletteRow> {
    let command = PaletteRow::command;
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
    for t in Theme::ALL {
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

  /// "Switch to `quest`" for every quest but the current lens, and back to
  /// all nodes when under one; "Rename" and "Delete" for every quest.
  fn quest_rows_for_palette(&self) -> Vec<PaletteRow> {
    let store = self.lock();
    let mut quests: Vec<_> = store
      .graph()
      .quests()
      .map(|q| (q.name.clone(), q.id))
      .collect();
    quests.sort();
    let row = PaletteRow::quest;
    let mut rows = Vec::new();
    for (name, id) in quests {
      if self.active_quest != Some(id) {
        rows.push(row(
          PaletteAct::Quest(Some(id)),
          format!("Switch to {name}"),
        ));
      }
      rows.push(row(
        PaletteAct::RenameQuest(id),
        format!("Rename quest {name}"),
      ));
      rows.push(row(
        PaletteAct::DeleteQuest(id),
        format!("Delete quest {name}"),
      ));
    }
    if self.active_quest.is_some() {
      rows.push(PaletteRow::quest(
        PaletteAct::Quest(None),
        "Switch to all nodes".into(),
      ));
    }
    rows
  }

  /// Quest membership for the selection: add it to each quest that does not
  /// claim it, take it out of each that does, or start a quest with it.
  fn membership_rows(&self) -> Vec<PaletteRow> {
    let Some(info) = self.selected_info() else {
      return Vec::new();
    };
    let name = self.name_draft.trim().to_string();
    let row = PaletteRow::quest;
    let mut rows: Vec<PaletteRow> = self
      .unclaimed_quests()
      .into_iter()
      .map(|(id, quest)| {
        row(PaletteAct::Claim(id), format!("Add {name} to {quest}"))
      })
      .collect();
    rows.extend(info.quests.into_iter().map(|(id, quest)| {
      row(
        PaletteAct::Unclaim(id),
        format!("Remove {name} from {quest}"),
      )
    }));
    rows.push(row(
      PaletteAct::NewQuestWith,
      format!("New quest with {name}"),
    ));
    rows
  }
}
