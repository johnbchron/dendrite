//! The quest lens and the completed lens, and which quests claim the
//! selection.

use base::{Event, NodeId, QuestId};

use super::{AppState, LiveEdit, cache::Lens, chrome::Popover};
use crate::focus::FieldKey;

impl AppState {
  /// Name and claim count of the active quest lens, or `None` in the global
  /// view. One lock for both, per the module note.
  pub fn active_quest_summary(&self) -> Option<(String, usize)> {
    let id = self.active_quest?;
    let store = self.lock();
    let quest = store.graph().quest(id)?;
    Some((quest.name.clone(), quest.claims.len()))
  }

  /// Create a quest called `name` and make it the lens. A blank name gives
  /// "New quest", and leaves the switcher open on its rename field.
  pub fn new_quest_named(&mut self, name: String) {
    let id = QuestId::new();
    let unnamed = name.trim().is_empty();
    self.commit(vec![Event::QuestCreated {
      quest: id,
      name: if unnamed { "New quest".into() } else { name },
    }]);
    if unnamed {
      self.rename_quest(id);
    } else {
      self.set_active_quest(Some(id));
    }
  }

  /// Rename `quest`: put the lens on it and open the switcher with the
  /// cursor in its name field (this replaces the switcher's own request for
  /// its search field).
  pub fn rename_quest(&mut self, quest: QuestId) {
    self.set_active_quest(Some(quest));
    self.open_picker();
    self.focus_requests.request(FieldKey::QuestName);
  }

  /// Delete `quest`. Its nodes stay, since a quest owns nothing; if it was
  /// the lens, the view goes back to every node. There is no confirmation:
  /// a toast offers Undo, as for deleting a node.
  pub fn delete_quest(&mut self, quest: QuestId) {
    let name = self.lock().graph().quest(quest).map(|q| q.name.clone());
    let Some(name) = name else { return };
    self.commit(vec![Event::QuestRemoved { quest }]);
    if self.active_quest == Some(quest) {
      // Not `set_active_quest`, which would close the switcher this may
      // have been deleted from.
      self.active_quest = None;
      self.sync_quest_draft();
    }
    let revision = self.lock().revision();
    self.toasts.show(format!("Deleted quest {name}"), revision);
  }

  /// Switch the active quest lens (or clear it for the global view), and
  /// collapse the switcher now that the choice is made.
  pub fn set_active_quest(&mut self, quest: Option<QuestId>) {
    self.live_edit = None;
    self.active_quest = quest;
    self.completed_lens = false;
    self.close(Popover::Quests);
    self.sync_quest_draft();
  }

  /// Switch to the completed lens: the completed trees, and nothing else.
  pub fn show_completed(&mut self) {
    self.set_active_quest(None);
    self.completed_lens = true;
  }

  /// Whether the completed lens is on.
  pub fn completed_lens(&self) -> bool {
    self.completed_lens
  }

  /// The lens in use, or `None` for the main view.
  pub(super) fn lens(&self) -> Option<Lens> {
    match self.active_quest {
      Some(quest) => Some(Lens::Quest(quest)),
      None => self.completed_lens.then_some(Lens::Completed),
    }
  }

  /// Refresh the quest rename buffer from the graph. Called whenever the
  /// active quest changes and after undo/redo, so the field never shows a
  /// name the graph no longer holds.
  pub(super) fn sync_quest_draft(&mut self) {
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
    self.commit_live(
      LiveEdit::QuestName(id),
      vec![Event::QuestRenamed { quest: id, name }],
    );
  }

  /// Enter in the quest rename field: the counterpart of
  /// [`AppState::finish_rename_selected`].
  pub fn finish_rename_quest(&mut self) {
    self.live_edit = None;
    self.sync_quest_draft();
  }

  /// Whether the inspector's list of quests to add the selection to is
  /// showing.
  pub fn quests_open(&self) -> bool {
    self.quests_open
  }

  /// Show or hide the inspector's list of quests to add the selection to.
  pub fn toggle_quests(&mut self) {
    self.quests_open = !self.quests_open;
  }

  /// The selected node, if a quest could claim it (it is not a formula
  /// condition).
  fn claimable_selection(&self) -> Option<NodeId> {
    let node = self.selected?;
    let store = self.lock();
    store
      .graph()
      .node(node)
      .is_some_and(|n| n.kind.claimable())
      .then_some(node)
  }

  /// The quests that do not claim the selected node, by name: the ones it
  /// could be added to. None for a node no quest can claim.
  pub fn unclaimed_quests(&self) -> Vec<(QuestId, String)> {
    let Some(node) = self.claimable_selection() else {
      return Vec::new();
    };
    let store = self.lock();
    let graph = store.graph();
    let claiming = base::claiming_quests(graph, node);
    let mut quests: Vec<(QuestId, String)> = graph
      .quests()
      .filter(|q| !claiming.contains(&q.id))
      .map(|q| (q.id, q.name.clone()))
      .collect();
    quests.sort_by(|a, b| a.1.cmp(&b.1));
    quests
  }

  /// Add the selected node to `quest` (claim it), unless no quest can.
  pub fn claim_selected(&mut self, quest: QuestId) {
    let Some(node) = self.claimable_selection() else {
      return;
    };
    self.quests_open = false;
    self.commit(vec![Event::QuestClaimed { quest, node }]);
  }

  /// Take the selected node out of `quest` (release its claim). Under that
  /// quest's lens the node may leave the view; it stays selected, so the
  /// inspector can put it back.
  pub fn unclaim_selected(&mut self, quest: QuestId) {
    let Some(node) = self.selected else { return };
    self.commit(vec![Event::QuestUnclaimed { quest, node }]);
  }

  /// Start a quest with the selected node in it, as one undo step: switch
  /// to its lens and put the cursor in its name, as a new quest does.
  /// Nothing, for a node no quest can claim.
  pub fn new_quest_with_selected(&mut self) {
    let Some(node) = self.claimable_selection() else {
      return;
    };
    let quest = QuestId::new();
    self.quests_open = false;
    self.commit(vec![
      Event::QuestCreated {
        quest,
        name: "New quest".into(),
      },
      Event::QuestClaimed { quest, node },
    ]);
    self.rename_quest(quest);
  }
}
