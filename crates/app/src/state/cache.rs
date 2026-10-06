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
//! passing, or a declared fact changing, reuses it. So do the completed
//! trees, which every view but their own leaves out, so the main view is
//! laid out without them.

use std::sync::{Arc, Mutex};

use base::{Completed, Derived, Facts, NodeId, QuestId};
use layout::{Layout, LayoutConfig};
use session::Session;

use super::AppState;
use crate::scene::CanvasScene;

/// Everything a [`CanvasScene`] is built from: the graph and facts
/// revisions (node states), the selection (highlighted), and the lens
/// (which nodes show, which dim).
pub(super) type SceneKey = (u64, u64, Option<NodeId>, Option<Lens>);

/// A view laid out on its own: a quest's, or the completed trees'.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Lens {
  /// The quest's scope.
  Quest(QuestId),
  /// The completed trees.
  Completed,
}

/// What a lens's layout is computed from: the graph revision and the lens.
type LensKey = (u64, Lens);

/// Whole-graph computations: derived state from the graph and the facts,
/// and layout from the graph alone.
pub(super) struct Derivations {
  /// The [`Session::revision`] these were computed at.
  revision: u64,
  /// The [`AppState::facts_revision`] the derived state was computed at.
  facts_revision: u64,
  /// The facts the derived state was computed from.
  pub(super) facts: Facts,
  /// Readiness, cycles, satisfaction.
  pub(super) derived: Derived,
  /// The completed trees; shared with the previous derivations when only
  /// the facts changed.
  pub(super) completed: Arc<Completed>,
  /// Ranks, ordering and the reversed edges of every node but the retired
  /// ones; shared like `completed`.
  pub(super) layout: Arc<Layout>,
}

/// The latest [`Derivations`] and canvas scene.
#[derive(Default)]
pub(super) struct Caches {
  derivations: Mutex<Option<Arc<Derivations>>>,
  /// The layout of the last quest lens drawn, and what it was made from.
  lens_layout: Memo<LensKey, Layout>,
  /// The last canvas scene and what it was built from. Handing the canvas
  /// the same `Arc` is how it knows it has nothing to re-measure.
  scene: Memo<SceneKey, CanvasScene>,
}

/// The last value built, and the key it was built for.
struct Memo<K, V>(Mutex<Option<(K, Arc<V>)>>);

impl<K, V> Default for Memo<K, V> {
  fn default() -> Self {
    Self(Mutex::default())
  }
}

impl<K: PartialEq, V> Memo<K, V> {
  /// The value for `key`: the last one if it was built for the same key,
  /// else a fresh one from `build`.
  fn get_or(&self, key: K, build: impl FnOnce() -> V) -> Arc<V> {
    let mut cache = self.0.lock().expect("cache mutex poisoned");
    if let Some((k, value)) = cache.as_ref()
      && *k == key
    {
      return value.clone();
    }
    let value = Arc::new(build());
    *cache = Some((key, value.clone()));
    value
  }
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
    let (completed, layout) = match cache.as_ref() {
      Some(d)
        if d.revision == revision && d.facts_revision == facts_revision =>
      {
        return d.clone();
      }
      Some(d) if d.revision == revision => {
        (d.completed.clone(), d.layout.clone())
      }
      _ => {
        let graph = store.graph();
        let completed = base::completed(graph);
        let live = graph.induced(|n| !completed.retired.contains(&n));
        let layout = Layout::compute(&live, &LayoutConfig::default());
        (Arc::new(completed), Arc::new(layout))
      }
    };
    let facts = facts();
    let fresh = Arc::new(Derivations {
      revision,
      facts_revision,
      derived: Derived::compute(store.graph(), &facts),
      facts,
      completed,
      layout,
    });
    *cache = Some(fresh.clone());
    fresh
  }

  /// The layout of `lens`: the subgraph it shows, laid out on its own,
  /// computed at most once per revision and lens. `completed` must be of
  /// the same revision.
  pub(super) fn lens_layout(
    &self,
    store: &Session,
    completed: &Completed,
    lens: Lens,
  ) -> Arc<Layout> {
    self.lens_layout.get_or((store.revision(), lens), || {
      let graph = store.graph();
      let sub = match lens {
        Lens::Quest(quest) => {
          let scope = base::scope(graph, quest);
          graph
            .induced(|n| scope.contains(n) && !completed.retired.contains(&n))
        }
        Lens::Completed => graph.induced(|n| completed.trees.contains(&n)),
      };
      Layout::compute(&sub, &LayoutConfig::default())
    })
  }

  /// The scene for `key`: the last one if it was built for the same key,
  /// else a fresh one from `build`.
  pub(super) fn scene(
    &self,
    key: SceneKey,
    build: impl FnOnce() -> CanvasScene,
  ) -> Arc<CanvasScene> {
    self.scene.get_or(key, build)
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
