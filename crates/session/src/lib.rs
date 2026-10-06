//! `session` — the editing session: the graph as it stands, and the undo
//! and redo stacks that walk it (PLAN §3).
//!
//! This is the half of persistence that has nothing to do with storage. It
//! folds [`base::Event`]s into a [`Graph`], groups them into undoable steps,
//! and appends every change — undo included — to an append-only log it
//! reaches through the [`Backend`] trait. Which log that is (SQLite on the
//! desktop, something else in a browser) it neither knows nor cares.
//!
//! The crate is pure: no I/O, no SQL, no platform. Everything that touches
//! a disk is behind [`Backend`].

mod history;
mod memory;

use base::{Event, Graph};
pub use history::Group;
pub use memory::Memory;

/// Anything the backing store can fail with.
///
/// Boxed rather than an associated type on [`Backend`]: that would make
/// [`Session`] generic, and the parameter would spread through every type
/// built on it — right out into the view tree. Nothing above this crate
/// inspects a storage error, it only reports one, so the concrete type buys
/// nothing and costs a great deal. A backend that wants its own error back
/// can downcast.
pub type Error = Box<dyn std::error::Error + Send + Sync>;

/// Where a session's events and preferences are kept.
///
/// Two groups of methods, both served by one backing store:
///
/// - the **log**: [`replay`](Backend::replay), [`append`](Backend::append) and
///   [`replace`](Backend::replace). It is append-only, and the log's order is
///   the session's history; `replace` exists solely so a rename typed one
///   keystroke at a time is logged once rather than once per character (see
///   [`Session::commit_amend`]).
/// - **preferences**: [`setting`](Backend::setting) and
///   [`set_setting`](Backend::set_setting). These are not graph data, so they
///   live beside the log rather than in it, and never reach the undo stack.
///
/// A backend that wants preferences somewhere else (browser local storage,
/// say, with the log in a database) implements both groups and delegates.
pub trait Backend {
  /// Every event in the log, oldest first, and the position of the last
  /// one (0 for an empty log).
  fn replay(&self) -> Result<(Vec<Event>, i64), Error>;

  /// How many events the log holds. It only ever grows, undo included.
  fn event_count(&self) -> Result<u64, Error>;

  /// Append `events` to the log, returning the position of the last one.
  /// Positions increase and are never reused.
  fn append(&mut self, events: &[Event]) -> Result<i64, Error>;

  /// Overwrite the event at `at` with `event`, which supersedes it.
  fn replace(&mut self, at: i64, event: &Event) -> Result<(), Error>;

  /// The preference stored under `key`, or `None` if it was never written.
  fn setting(&self, key: &str) -> Result<Option<String>, Error>;

  /// Write a preference, replacing any previous value.
  fn set_setting(&self, key: &str, value: &str) -> Result<(), Error>;
}

/// An editing session: the graph, the undo and redo stacks, and the log
/// everything is written through.
///
/// Each entry on a stack is one *group* — the set of events committed
/// together — so a single `undo`/`redo` reverses a whole user action.
pub struct Session {
  /// `Send`, so a `Mutex<Session>` is `Send + Sync` and can live in an app
  /// state a UI toolkit requires to be both. Not `Sync`: SQLite's
  /// connection is not, which is what the mutex is there for.
  backend: Box<dyn Backend + Send>,
  graph: Graph,
  /// Inverse batches, newest last, each with the label of the group it
  /// undoes. Popping one and applying it undoes the most recent group.
  undo: Vec<Group>,
  /// Batches that re-apply undone groups, newest last, with their labels.
  redo: Vec<Group>,
  /// Position of the newest event in the log (0 for an empty log).
  last_seq: i64,
  /// Bumped every time the graph changes, so callers can cache anything
  /// derived from it and know when to recompute.
  revision: u64,
  /// The last event of the newest undo group and its position, while that
  /// group can still be amended; cleared by undo and redo, whose events
  /// are history and must never be rewritten.
  tail: Option<(i64, Event)>,
}

impl Session {
  /// Open a session on `backend`, replaying its log into the graph.
  pub fn new(backend: Box<dyn Backend + Send>) -> Result<Self, Error> {
    let mut graph = Graph::new();
    let (events, last_seq) = backend.replay()?;
    for event in &events {
      event.apply(&mut graph);
    }
    Ok(Session {
      last_seq,
      backend,
      graph,
      undo: Vec::new(),
      redo: Vec::new(),
      revision: 0,
      tail: None,
    })
  }

  /// The current graph.
  pub fn graph(&self) -> &Graph {
    &self.graph
  }

  /// A counter that changes whenever [`Session::graph`] does (commit,
  /// amend, undo, redo) and never otherwise. Equal revisions mean an equal
  /// graph.
  pub fn revision(&self) -> u64 {
    self.revision
  }

  /// How many events the log holds. It only ever grows: undo appends the
  /// inverse rather than deleting anything.
  pub fn event_count(&self) -> Result<u64, Error> {
    self.backend.event_count()
  }

  /// Read a UI preference.
  ///
  /// Preferences live outside the event log on purpose: they are not graph
  /// data, so changing one must not appear on the undo stack or in the
  /// audit trail.
  pub fn setting(&self, key: &str) -> Result<Option<String>, Error> {
    self.backend.setting(key)
  }

  /// Write a UI preference, replacing any previous value.
  pub fn set_setting(&self, key: &str, value: &str) -> Result<(), Error> {
    self.backend.set_setting(key, value)
  }
}
