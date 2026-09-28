//! Application state and the command layer that turns UI gestures into
//! [`base::Event`]s committed through the editing [`Session`] (PLAN §3, §4).
//!
//! `AppState` owns the store (the persistent global graph) and the ephemeral
//! view state — the current selection, the active quest lens, and the rename
//! draft. Everything the canvas and inspector render is derived here from
//! the store on demand.
//!
//! The store is wrapped in a [`Mutex`] because Xilem's `WidgetView` bound is
//! `Send + Sync` and the SQLite backend's `Connection` is `!Sync`. That is
//! the backend's constraint, not the session's — a `Sync` backend would not
//! need the lock. The app is single-threaded, so it is uncontended; each
//! read method takes it exactly once (the `std` mutex is not re-entrant).
//!
//! The methods are grouped by what they serve:
//!
//! - `cache` and `scene`: whole-graph derivations and the canvas scene.
//! - `selection` and `navigate`: the selected node, as the inspector shows it,
//!   and moving the selection with the arrows.
//! - `edit`: commands that change the graph, and undo/redo.
//! - `link`: link mode, adding requirements from the canvas or a search.
//! - `quests` and `switcher`: the quest lens, quest membership, and the quest
//!   switcher.
//! - `now`: the Now tray.
//! - `palette`: the command palette.
//! - `chrome`, `toast` and `commands`: panels and popovers, the undo toast, and
//!   the key map's commands.

mod cache;
mod chrome;
mod commands;
mod edit;
mod facts;
mod link;
mod navigate;
mod now;
mod palette;
mod quests;
mod scene;
mod selection;
mod switcher;
#[cfg(test)]
mod tests;
mod toast;

use std::sync::{Mutex, MutexGuard};

use base::{Event, NodeId, QuestId};
use jiff::Timestamp;
use session::Session;

use self::{
  cache::Caches,
  chrome::{PanelWidth, Popover},
  facts::Declared,
  palette::Recent,
  toast::Toasts,
};
pub use self::{
  facts::SAFETY_TICK,
  link::AtomOffer,
  now::SoonItem,
  palette::{PaletteRow, RowKind},
  selection::{EdgeRow, Reason},
  switcher::QuestChoice,
  toast::Toast,
};
use crate::{
  camera::Camera,
  focus::FocusRequests,
  formula::{Clock, SystemClock},
  query::Query,
  theme::Theme,
};

/// The whole application's state.
pub struct AppState {
  store:              Mutex<Session>,
  /// Currently selected node, if any.
  pub selected:       Option<NodeId>,
  /// Which box of the selected node it was reached through: a copy of a
  /// shared condition, when one was clicked or stepped onto. `None` means
  /// the node's first box.
  selected_copy:      Option<NodeId>,
  /// Active quest lens; `None` means the global "all nodes" view (PLAN §5).
  pub active_quest:   Option<QuestId>,
  /// Editable name buffer for the selected node.
  pub name_draft:     String,
  /// Editable name buffer for the active quest, shown in the switcher.
  pub quest_draft:    String,
  /// The latest request for the canvas camera (fit, reveal a node).
  camera:             Camera,
  /// The inspector's width, and its anchor while a divider drag is on.
  panel:              PanelWidth,
  /// When set, the next canvas click picks a requirement target for the
  /// selection instead of changing the selection.
  linking:            bool,
  /// Filter text for the requirement picker — the fallback path for targets
  /// that are not on the canvas (e.g. outside the active quest's scope).
  pub link_filter:    String,
  /// The popover (or the palette) that is open, if any.
  popover:            Option<Popover>,
  /// What has been typed into the quest switcher, and its highlight.
  quest_query:        Query,
  /// The palette every painted surface reads its colours from.
  theme:              &'static Theme,
  /// Whether the Now tray is open (rather than collapsed to its pill).
  now_open:           bool,
  /// Whether the inspector's "more actions" list is showing.
  more_open:          bool,
  /// Whether the inspector's list of quests to add the selection to is
  /// showing.
  quests_open:        bool,
  /// The notice on screen, if any.
  toasts:             Toasts,
  /// What has been typed into the palette, and its highlight.
  palette_query:      Query,
  /// Whether the palette was opened to search nodes only.
  palette_nodes_only: bool,
  /// Recently selected nodes, for the empty palette.
  recent:             Recent,
  /// Fields to focus once the views catch up (see [`FocusRequests`]).
  focus_requests:     FocusRequests,
  /// The canvas zoom as a whole percentage, as last reported by the canvas.
  zoom_percent:       u32,
  /// The field whose keystrokes are currently being committed, if the last
  /// commit came from one. The next keystroke in the same field amends that
  /// undo group rather than opening a new one.
  live_edit:          Option<LiveEdit>,
  /// Derived state, layout and the canvas scene for the store's current
  /// revision.
  caches:             Caches,
  /// Where the time comes from.
  clock:              Box<dyn Clock>,
  /// What I have declared about the world, mirrored from preferences.
  declared:           Declared,
  /// Bumped whenever the facts may have changed; see
  /// [`AppState::facts_revision`].
  facts_revision:     u64,
  /// When [`AppState::tick`] last looked at the clock.
  checked_at:         Timestamp,
}

/// A text field that commits as it is typed in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LiveEdit {
  /// The inspector's name field, for this node.
  NodeName(NodeId),
  /// The quest switcher's rename field, for this quest.
  QuestName(QuestId),
}

impl AppState {
  /// Wrap `store`, restoring the palette and the declared facts it
  /// records, on the system clock.
  pub fn new(store: Session) -> Self { Self::with_clock(store, SystemClock) }

  /// Wrap `store`, reading the time from `clock`.
  pub fn with_clock(store: Session, clock: impl Clock + 'static) -> Self {
    let declared = Declared::load(&store);
    let checked_at = clock.now();
    let mut state = Self {
      store: Mutex::new(store),
      selected: None,
      selected_copy: None,
      active_quest: None,
      name_draft: String::new(),
      quest_draft: String::new(),
      camera: Camera::default(),
      panel: PanelWidth::default(),
      linking: false,
      link_filter: String::new(),
      popover: None,
      quest_query: Query::default(),
      theme: Theme::DEFAULT,
      now_open: false,
      more_open: false,
      quests_open: false,
      toasts: Toasts::default(),
      palette_query: Query::default(),
      palette_nodes_only: false,
      recent: Recent::default(),
      focus_requests: FocusRequests::default(),
      zoom_percent: 100,
      live_edit: None,
      caches: Caches::default(),
      clock: Box::new(clock),
      declared,
      facts_revision: 0,
      checked_at,
    };
    state.theme = state.stored_theme();
    state
  }

  fn lock(&self) -> MutexGuard<'_, Session> {
    self.store.lock().expect("store mutex poisoned")
  }

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
}
