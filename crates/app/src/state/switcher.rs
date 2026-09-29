//! The quest switcher: a searchable list of quests to put the lens on.

use base::QuestId;

use super::{AppState, chrome::Popover};
use crate::{
  focus::FieldKey,
  query::{self, Query},
};

/// The completed lens's name.
pub const COMPLETED: &str = "Completed";

/// What a quest switcher row does when chosen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuestChoice {
  /// Leave the lens: show every node but the completed trees.
  All,
  /// Show the completed trees.
  Completed,
  /// Switch to this quest.
  Quest(QuestId),
  /// Create a quest, named by the query if there is one.
  New,
}

/// One row of the quest switcher.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestRow {
  /// What choosing it does.
  pub choice:  QuestChoice,
  /// What it says.
  pub label:   String,
  /// Whether it is the lens in use now.
  pub current: bool,
}

impl AppState {
  /// Whether the quest switcher is expanded.
  pub fn picker_open(&self) -> bool { self.popover == Some(Popover::Quests) }

  /// Expand or collapse the quest switcher.
  pub fn toggle_picker(&mut self) {
    if self.picker_open() {
      self.close_popovers();
    } else {
      self.open_picker();
    }
  }

  /// Open the quest switcher with an empty query.
  pub(super) fn open_picker(&mut self) {
    self.close_popovers();
    self.quest_query = Query::default();
    self.popover = Some(Popover::Quests);
    self.focus_requests.request(FieldKey::QuestSearch);
  }

  /// The quest switcher's rows for the current query: "All nodes",
  /// "Completed", the quests that match (best match first, then by name),
  /// and "New quest", which takes the query as its name.
  pub fn quest_rows(&self) -> Vec<QuestRow> {
    let text = self.quest_query.text.trim();
    let mut rows = Vec::new();
    if query::score(text, "All nodes").is_some() {
      rows.push(QuestRow {
        choice:  QuestChoice::All,
        label:   "All nodes".into(),
        current: self.lens().is_none(),
      });
    }
    if query::score(text, COMPLETED).is_some() {
      rows.push(QuestRow {
        choice:  QuestChoice::Completed,
        label:   COMPLETED.into(),
        current: self.completed_lens,
      });
    }
    let store = self.lock();
    let mut quests: Vec<(u32, String, QuestId)> = store
      .graph()
      .quests()
      .filter_map(|q| {
        query::score(text, &q.name).map(|sc| (sc, q.name.clone(), q.id))
      })
      .collect();
    quests.sort();
    rows.extend(quests.into_iter().map(|(_, name, id)| QuestRow {
      choice:  QuestChoice::Quest(id),
      label:   name,
      current: self.active_quest == Some(id),
    }));
    rows.push(QuestRow {
      choice:  QuestChoice::New,
      label:   if text.is_empty() {
        "New quest".into()
      } else {
        format!("New quest \u{201c}{text}\u{201d}")
      },
      current: false,
    });
    rows
  }

  /// The quest switcher's query, for its search box and highlight.
  pub fn quest_query(&self) -> &Query { &self.quest_query }

  /// The quest switcher's search field changed.
  pub fn set_quest_text(&mut self, text: String) {
    self.quest_query.set_text(text);
  }

  /// Choose the switcher's highlighted row (Enter).
  pub fn accept_quest(&mut self) {
    let rows = self.quest_rows();
    let pick = rows[self.quest_query.highlighted(rows.len())].choice;
    self.choose_quest(pick);
  }

  /// Act on a quest switcher row.
  pub fn choose_quest(&mut self, choice: QuestChoice) {
    match choice {
      QuestChoice::All => self.set_active_quest(None),
      QuestChoice::Completed => self.show_completed(),
      QuestChoice::Quest(id) => self.set_active_quest(Some(id)),
      QuestChoice::New => {
        let name = self.quest_query.text.trim().to_string();
        self.new_quest_named(name);
      }
    }
  }
}
