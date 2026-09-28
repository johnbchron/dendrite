//! Whole-graph computations, kept for as long as the graph they were made
//! from.
//!
//! Derived state and layout are whole-graph computations, and the view asks
//! for them on every rebuild, most of which (a keystroke in a filter, a
//! panel drag) change nothing. A quest lens is laid out on its own, as the
//! subgraph it shows, so that is kept too, for as long as the graph and the
//! lens stay the same.
//!
//! Derived state also depends on the facts, so it is kept per graph
//! revision *and* facts revision; layout depends on the graph alone, so time
//! passing, or a declared fact changing, reuses it.

use std::sync::{Arc, Mutex};

use base::{Derived, Facts, NodeId, QuestId};
use layout::{Layout, LayoutConfig};
use session::Session;

use super::AppState;
use crate::scene::CanvasScene;

/// Everything a [`CanvasScene`] is built from: the graph and facts
/// revisions (node states), the selection (highlighted), and the lens
/// (which nodes show, which dim).
pub(super) type SceneKey = (u64, u64, Option<NodeId>, Option<QuestId>);

/// What a lens's layout is computed from: the graph revision and the quest.
type LensKey = (u64, QuestId);

/// Whole-graph computations: derived state from the graph and the facts,
/// and layout from the graph alone.
pub(super) struct Derivations {
  /// The [`Session::revision`] these were computed at.
  revision:           u64,
  /// The [`AppState::facts_revision`] the derived state was computed at.
  facts_revision:     u64,
  /// The facts the derived state was computed from.
  pub(super) facts:   Facts,
  /// Readiness, cycles, satisfaction.
  pub(super) derived: Derived,
  /// Ranks, ordering and the reversed edges; shared with the previous
  /// derivations when only the facts changed.
  pub(super) layout:  Arc<Layout>,
}

/// The latest [`Derivations`] and canvas scene.
#[derive(Default)]
pub(super) struct Caches {
  derivations: Mutex<Option<Arc<Derivations>>>,
  /// The layout of the last quest lens drawn, and what it was made from.
  lens_layout: Mutex<Option<(LensKey, Arc<Layout>)>>,
  /// The last canvas scene and what it was built from. Handing the canvas
  /// the same `Arc` is how it knows it has nothing to re-measure.
  scene:       Mutex<Option<(SceneKey, Arc<CanvasScene>)>>,
}

impl Caches {
  /// Derivations for `store`'s current graph at `facts_revision`: derived
  /// state computed at most once per pair of revisions, from the `facts`
  /// asked for then, and layout at most once per graph revision.
  pub(super) fn derivations(
    &self,
    store: &Session,
    facts_revision: u64,
    facts: impl FnOnce() -> Facts,
  ) -> Arc<Derivations> {
    let mut cache = self.derivations.lock().expect("cache mutex poisoned");
    let revision = store.revision();
    let layout = match cache.as_ref() {
      Some(d)
        if d.revision == revision && d.facts_revision == facts_revision =>
      {
        return d.clone();
      }
      Some(d) if d.revision == revision => d.layout.clone(),
      _ => Arc::new(Layout::compute(store.graph(), &LayoutConfig::default())),
    };
    let facts = facts();
    let fresh = Arc::new(Derivations {
      revision,
      facts_revision,
      derived: Derived::compute(store.graph(), &facts),
      facts,
      layout,
    });
    *cache = Some(fresh.clone());
    fresh
  }

  /// The layout of `quest`'s lens: the subgraph it shows, laid out on its
  /// own, computed at most once per revision and quest.
  pub(super) fn lens_layout(
    &self,
    store: &Session,
    quest: QuestId,
  ) -> Arc<Layout> {
    let key = (store.revision(), quest);
    let mut cache = self.lens_layout.lock().expect("cache mutex poisoned");
    if let Some((k, layout)) = cache.as_ref()
      && *k == key
    {
      return layout.clone();
    }
    let graph = store.graph();
    let scope = base::scope(graph, quest);
    let sub = graph.induced(|n| scope.contains(n));
    let layout = Arc::new(Layout::compute(&sub, &LayoutConfig::default()));
    *cache = Some((key, layout.clone()));
    layout
  }

  /// The scene for `key`: the last one if it was built for the same key,
  /// else a fresh one from `build`.
  pub(super) fn scene(
    &self,
    key: SceneKey,
    build: impl FnOnce() -> CanvasScene,
  ) -> Arc<CanvasScene> {
    let mut cache = self.scene.lock().expect("cache mutex poisoned");
    if let Some((k, scene)) = cache.as_ref()
      && *k == key
    {
      return scene.clone();
    }
    let scene = Arc::new(build());
    *cache = Some((key, scene.clone()));
    scene
  }
}

impl AppState {
  /// Derived state and layout for `store`'s current graph under the
  /// current facts; see [`Caches::derivations`].
  pub(super) fn derivations(&self, store: &Session) -> Arc<Derivations> {
    self
      .caches
      .derivations(store, self.facts_revision, || self.facts())
  }
}
