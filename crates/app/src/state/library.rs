//! The library: every place, context, resource and schedule in one popover,
//! whether or not the selection points at it.
//!
//! The inspector's referent card only reaches a referent through a formula
//! condition that uses it. Here each one can be renamed, deleted once
//! nothing uses it, or followed to the conditions that do; places and
//! contexts, which need nothing but a name, can be added outright. Pruning
//! (see [`base::prune`]) is offered here too.

use base::{
  Atom, ContextId, Event, Graph, NodeId, PlaceId, ResourceId, ScheduleId,
};

use super::{AppState, LiveEdit, chrome::Popover};
use crate::{focus::FieldKey, formula::describe};

/// Which kind of referent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum RefKind {
  /// Somewhere I can be.
  Place,
  /// A circumstance I toggle.
  Context,
  /// Something countable I have some of.
  Resource,
  /// A named set of windows in time.
  Schedule,
}

impl RefKind {
  /// Every kind, in the order the library lists them.
  pub const ALL: [RefKind; 4] = [
    RefKind::Place,
    RefKind::Context,
    RefKind::Resource,
    RefKind::Schedule,
  ];

  /// The kind in a sentence: "place".
  pub fn noun(self) -> &'static str {
    match self {
      RefKind::Place => "place",
      RefKind::Context => "context",
      RefKind::Resource => "resource",
      RefKind::Schedule => "schedule",
    }
  }

  /// The library's heading for the kind: "Places".
  pub fn heading(self) -> &'static str {
    match self {
      RefKind::Place => "Places",
      RefKind::Context => "Contexts",
      RefKind::Resource => "Resources",
      RefKind::Schedule => "Schedules",
    }
  }

  /// Whether the library can add one of this kind from a name alone. A
  /// resource needs a unit and a schedule its windows, so those come from
  /// typing a condition ("$50", "during business hours").
  pub fn addable(self) -> bool {
    matches!(self, RefKind::Place | RefKind::Context)
  }
}

/// One referent, by id.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RefKey {
  /// A place.
  Place(PlaceId),
  /// A context.
  Context(ContextId),
  /// A resource.
  Resource(ResourceId),
  /// A schedule.
  Schedule(ScheduleId),
}

impl RefKey {
  /// The referent `atom` points at, if it points at one.
  pub fn of(atom: &Atom) -> Option<Self> {
    match atom {
      Atom::At { place } => Some(RefKey::Place(*place)),
      Atom::In { context } => Some(RefKey::Context(*context)),
      Atom::Has { resource, .. } => Some(RefKey::Resource(*resource)),
      Atom::Within { schedule } => Some(RefKey::Schedule(*schedule)),
      Atom::After { .. } | Atom::Before { .. } | Atom::Free { .. } => None,
    }
  }

  /// Which kind it is.
  pub fn kind(self) -> RefKind {
    match self {
      RefKey::Place(_) => RefKind::Place,
      RefKey::Context(_) => RefKind::Context,
      RefKey::Resource(_) => RefKind::Resource,
      RefKey::Schedule(_) => RefKind::Schedule,
    }
  }

  /// The raw id, for keying live edits.
  pub(super) fn raw(self) -> u128 {
    match self {
      RefKey::Place(id) => id.to_u128(),
      RefKey::Context(id) => id.to_u128(),
      RefKey::Resource(id) => id.to_u128(),
      RefKey::Schedule(id) => id.to_u128(),
    }
  }

  /// Its name in `graph`, if it still exists.
  pub(super) fn name(self, graph: &Graph) -> Option<String> {
    match self {
      RefKey::Place(id) => graph.place(id).map(|p| p.name.clone()),
      RefKey::Context(id) => graph.context(id).map(|c| c.name.clone()),
      RefKey::Resource(id) => graph.resource(id).map(|r| r.name.clone()),
      RefKey::Schedule(id) => graph.schedule(id).map(|s| s.name.clone()),
    }
  }

  /// The event that renames it to `name`, or `None` when it is gone or
  /// already has that name.
  pub(super) fn rename(self, graph: &Graph, name: String) -> Option<Event> {
    if self.name(graph).is_none_or(|n| n == name) {
      return None;
    }
    Some(match self {
      RefKey::Place(place) => Event::PlaceChanged {
        place,
        name,
        within: graph.place(place)?.within,
      },
      RefKey::Context(context) => Event::ContextRenamed { context, name },
      RefKey::Resource(resource) => Event::ResourceRenamed { resource, name },
      RefKey::Schedule(schedule) => Event::ScheduleChanged {
        schedule,
        name,
        spans: graph.schedule(schedule)?.spans.clone(),
      },
    })
  }

  /// The event that removes it.
  fn removal(self) -> Event {
    match self {
      RefKey::Place(place) => Event::PlaceRemoved { place },
      RefKey::Context(context) => Event::ContextRemoved { context },
      RefKey::Resource(resource) => Event::ResourceRemoved { resource },
      RefKey::Schedule(schedule) => Event::ScheduleRemoved { schedule },
    }
  }
}

/// One referent as the library lists it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibraryItem {
  /// Which referent.
  pub key:  RefKey,
  /// Its name.
  pub name: String,
  /// What it holds, when there is something to say: a balance, how many
  /// windows, "Here", "On".
  pub note: Option<String>,
  /// The formula conditions that point at it, by name order.
  pub uses: Vec<NodeId>,
}

/// One kind's referents, by name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LibrarySection {
  /// Which kind.
  pub kind:  RefKind,
  /// Its referents.
  pub items: Vec<LibraryItem>,
}

impl AppState {
  /// Whether the library popover is open.
  pub fn library_open(&self) -> bool { self.popover == Some(Popover::Library) }

  /// Open or close the library, closing any other popover.
  pub fn toggle_library(&mut self) {
    let open = !self.library_open();
    self.close_popovers();
    if open {
      self.popover = Some(Popover::Library);
    }
  }

  /// Every referent, a section per kind in [`RefKind::ALL`] order.
  pub fn library(&self) -> Vec<LibrarySection> {
    let store = self.lock();
    let graph = store.graph();
    let uses = |key: RefKey| {
      let mut nodes: Vec<(String, NodeId)> = graph
        .nodes()
        .filter(|n| n.kind.atom().and_then(RefKey::of) == Some(key))
        .map(|n| (describe::node_name(graph, n, self.today()), n.id))
        .collect();
      nodes.sort();
      nodes.into_iter().map(|(_, id)| id).collect()
    };
    let item = |key: RefKey, name: &str, note: Option<String>| LibraryItem {
      key,
      name: name.to_string(),
      note,
      uses: uses(key),
    };
    let mut sections: Vec<LibrarySection> = RefKind::ALL
      .into_iter()
      .map(|kind| {
        let items = match kind {
          RefKind::Place => graph
            .places()
            .map(|p| {
              let here = self.place() == Some(p.id);
              item(RefKey::Place(p.id), &p.name, here.then(|| "Here".into()))
            })
            .collect(),
          RefKind::Context => graph
            .contexts()
            .map(|c| {
              let on = self.active_contexts().contains(&c.id);
              item(RefKey::Context(c.id), &c.name, on.then(|| "On".into()))
            })
            .collect(),
          RefKind::Resource => graph
            .resources()
            .map(|r| {
              let note = describe::amount(r, r.balance);
              item(RefKey::Resource(r.id), &r.name, Some(note))
            })
            .collect(),
          RefKind::Schedule => graph
            .schedules()
            .map(|s| {
              let note = match s.spans.len() {
                0 => "No windows".to_string(),
                1 => "1 window".to_string(),
                n => format!("{n} windows"),
              };
              item(RefKey::Schedule(s.id), &s.name, Some(note))
            })
            .collect(),
        };
        LibrarySection { kind, items }
      })
      .collect();
    for section in &mut sections {
      section.items.sort_by(|a, b| {
        a.name.cmp(&b.name).then(a.key.raw().cmp(&b.key.raw()))
      });
    }
    sections
  }

  /// The referent whose name is being edited in the library, if any.
  pub fn library_editing(&self) -> Option<RefKey> { self.library_edit }

  /// The library's name field's text.
  pub fn library_draft(&self) -> &str { &self.library_draft }

  /// Open the library with the cursor in `key`'s name.
  pub fn rename_referent(&mut self, key: RefKey) {
    let Some(name) = key.name(self.lock().graph()) else {
      return;
    };
    self.popover = Some(Popover::Library);
    self.live_edit = None;
    self.library_edit = Some(key);
    self.library_draft = name;
    self.focus_requests.request(FieldKey::LibraryName);
  }

  /// The library's name field changed: keep the text as typed and rename
  /// the referent to its trimmed form, one undo step per field.
  pub fn set_library_draft(&mut self, text: String) {
    self.library_draft = text;
    let Some(key) = self.library_edit else { return };
    let name = self.library_draft.trim().to_string();
    if name.is_empty() {
      return;
    }
    let Some(event) = key.rename(self.lock().graph(), name) else {
      return;
    };
    self.commit_live(LiveEdit::ReferentName(key.raw()), vec![event]);
    // The inspector's card may show the same referent.
    self.reset_drafts();
  }

  /// Enter in the library's name field, or clicking away: stop editing.
  pub fn finish_library_rename(&mut self) {
    self.live_edit = None;
    self.library_edit = None;
    self.library_draft.clear();
  }

  /// Add a place or context called "New place" (or "New place 2"…) and
  /// put the cursor in its name.
  pub fn new_referent(&mut self, kind: RefKind) { self.add_referent(kind); }

  /// Add a place I am at now, or a context that is on now, from the Now
  /// tray, and put the cursor in its name.
  pub fn declare_new(&mut self, kind: RefKind) {
    match self.add_referent(kind) {
      Some(RefKey::Place(place)) => {
        self.place_picker = false;
        self.set_place(Some(place));
      }
      Some(RefKey::Context(context)) => self.toggle_context(context),
      _ => {}
    }
  }

  /// [`new_referent`](Self::new_referent), saying which it added.
  fn add_referent(&mut self, kind: RefKind) -> Option<RefKey> {
    let (key, event) = {
      let store = self.lock();
      let graph = store.graph();
      let base = format!("New {}", kind.noun());
      let taken = |name: &str| match kind {
        RefKind::Place => graph.places().any(|p| p.name == name),
        RefKind::Context => graph.contexts().any(|c| c.name == name),
        _ => false,
      };
      let name = std::iter::once(base.clone())
        .chain((2..).map(|n| format!("{base} {n}")))
        .find(|n| !taken(n))
        .unwrap_or(base);
      match kind {
        RefKind::Place => {
          let place = PlaceId::new();
          (RefKey::Place(place), Event::PlaceDefined {
            place,
            name,
            within: None,
          })
        }
        RefKind::Context => {
          let context = ContextId::new();
          (RefKey::Context(context), Event::ContextDefined {
            context,
            name,
          })
        }
        RefKind::Resource | RefKind::Schedule => return None,
      }
    };
    self.commit(vec![event]);
    self.rename_referent(key);
    Some(key)
  }

  /// Delete `key`, unless a formula condition still points at it: the
  /// library only offers deleting what nothing uses, so no condition is
  /// left reading "Unknown place".
  pub fn delete_referent(&mut self, key: RefKey) {
    let (name, used) = {
      let store = self.lock();
      let graph = store.graph();
      let used = graph
        .nodes()
        .any(|n| n.kind.atom().and_then(RefKey::of) == Some(key));
      (key.name(graph), used)
    };
    let Some(name) = name.filter(|_| !used) else {
      return;
    };
    if self.library_edit == Some(key) {
      self.finish_library_rename();
    }
    self.commit(vec![key.removal()]);
    // A deleted place or context is no longer where I am or what is on.
    match key {
      RefKey::Place(place) if self.place() == Some(place) => {
        self.set_place(None);
      }
      RefKey::Context(context) if self.active_contexts().contains(&context) => {
        self.toggle_context(context);
      }
      _ => {}
    }
    let revision = self.lock().revision();
    self
      .toasts
      .show(format!("Deleted {} {name}", key.kind().noun()), revision);
  }

  /// Select a condition that uses `key`, closing the library to show it.
  pub fn show_use(&mut self, node: NodeId) {
    self.close(Popover::Library);
    self.reveal(node);
  }
}
