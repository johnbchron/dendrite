//! Whole-graph computations, kept for as long as the graph they were made
//! from.
//!
//! Derived state and layout are whole-graph computations, and the view asks
//! for them on every rebuild, most of which (a keystroke in a filter, a
//! panel drag) change nothing. A quest lens is laid out on its own, as the
//! subgraph it shows, so that is kept too, for as long as the graph and the
//! lens stay the same.

use std::sync::{Arc, Mutex};

use base::{Derived, NodeId, QuestId};
use db::Store;
use layout::{Layout, LayoutConfig};

use super::AppState;
use crate::canvas::CanvasScene;

/// Everything a [`CanvasScene`] is built from: the graph revision, the
/// selection (highlighted), and the lens (which nodes show, which dim).
pub(super) type SceneKey = (u64, Option<NodeId>, Option<QuestId>);

/// What a lens's layout is computed from: the graph revision and the quest.
type LensKey = (u64, QuestId);

/// Whole-graph computations that depend only on the graph.
pub(super) struct Derivations {
  /// The [`Store::revision`] these were computed at.
  revision:           u64,
  /// Readiness, cycles, satisfaction.
  pub(super) derived: Derived,
  /// Ranks, ordering and the reversed edges.
  pub(super) layout:  Layout,
}

impl Derivations {
  /// Compute everything for `store`'s current graph.
  fn compute(store: &Store) -> Self {
    let graph = store.graph();
    Self {
      revision: store.revision(),
      derived:  Derived::compute(graph),
      layout:   Layout::compute(graph, &LayoutConfig::default()),
    }
  }
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
  /// Derivations for `store`'s current graph, computed at most once per
  /// revision.
  pub(super) fn derivations(&self, store: &Store) -> Arc<Derivations> {
    let mut cache = self.derivations.lock().expect("cache mutex poisoned");
    if let Some(d) = cache.as_ref()
      && d.revision == store.revision()
    {
      return d.clone();
    }
    let fresh = Arc::new(Derivations::compute(store));
    *cache = Some(fresh.clone());
    fresh
  }

  /// The layout of `quest`'s lens: the subgraph it shows, laid out on its
  /// own, computed at most once per revision and quest.
  pub(super) fn lens_layout(
    &self,
    store: &Store,
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
  /// Derived state and layout for `store`'s current graph, computed at most
  /// once per revision.
  pub(super) fn derivations(&self, store: &Store) -> Arc<Derivations> {
    self.caches.derivations(store)
  }
}
