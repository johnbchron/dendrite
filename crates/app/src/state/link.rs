//! Link mode: adding requirements to the selection by clicking nodes on the
//! canvas, or by searching for them.

use std::collections::HashSet;

use base::{EdgeId, EdgeKind, NodeId};

use super::AppState;
use crate::{
  formula::{
    describe,
    phrase::{self, Offer},
  },
  query,
  scene::LinkMode,
};

/// A formula the requirement search offers for what was typed.
#[derive(Clone, Debug, PartialEq)]
pub struct AtomOffer {
  /// The reading of the phrase.
  pub offer:   Offer,
  /// How many nodes already require this atom: it exists and choosing it
  /// links to it ("At Home — used by 6"). Zero for a new one.
  pub used_by: usize,
}

/// Most rows the requirement picker will ever show. The panel must not grow
/// with the graph; anything beyond this is narrowed with the filter instead.
pub(super) const LINK_PICKER_MAX: usize = 6;

impl AppState {
  /// Whether the canvas is armed to pick a requirement target.
  pub fn is_linking(&self) -> bool { self.linking }

  /// Arm the canvas: node clicks add requirements to the selection rather
  /// than moving the selection. No-op without a selection, and a no-op if
  /// already armed (so refocusing the search keeps what was typed).
  pub fn begin_link(&mut self) {
    if self.linking {
      return;
    }
    self.linking = self.selected.is_some();
    self.link_filter.clear();
  }

  /// What the canvas needs to show link mode: the node gaining requirements,
  /// the nodes a click cannot add (itself and those already required), and
  /// those that would close a cycle (everything that already requires it,
  /// directly or not). `None` when not linking.
  pub fn link_mode(&self) -> Option<LinkMode> {
    if !self.linking {
      return None;
    }
    let source = self.selected?;
    let store = self.lock();
    let graph = store.graph();
    let mut taken: HashSet<NodeId> =
      graph.requirements_of(source).map(|e| e.to).collect();
    taken.insert(source);
    let mut closes_cycle = HashSet::new();
    let mut stack = vec![source];
    while let Some(n) = stack.pop() {
      for e in graph.dependents_of(n) {
        if closes_cycle.insert(e.from) {
          stack.push(e.from);
        }
      }
    }
    closes_cycle.retain(|n| !taken.contains(n));
    let name = graph
      .node(source)
      .map(|n| describe::node_name(graph, n, self.today()))
      .unwrap_or_default();
    Some(LinkMode {
      source,
      name,
      taken,
      closes_cycle,
    })
  }

  /// Disarm without linking anything.
  pub fn cancel_link(&mut self) {
    self.linking = false;
    self.link_filter.clear();
  }

  /// A click on the canvas. While armed this consumes the click to build a
  /// requirement edge and disarms, keeping the selection put; with `keep`
  /// (Shift held) it stays armed so several can be added in a row. Clicking
  /// empty space means "never mind". Unarmed, it moves the selection, to
  /// the `copy` of the node that was clicked.
  pub fn canvas_click(
    &mut self,
    node: Option<NodeId>,
    copy: Option<NodeId>,
    keep: bool,
  ) {
    if self.linking {
      match node {
        Some(target) => {
          self.add_requirement(target);
          if !keep {
            self.cancel_link();
          }
        }
        None => self.cancel_link(),
      }
      return;
    }
    self.select(node);
    self.selected_copy = copy;
  }

  /// Enter in the requirement search: link the best match and disarm.
  pub fn link_best_match(&mut self) {
    if let Some((target, _)) = self.candidate_requirements().0.first() {
      let target = *target;
      self.add_requirement(target);
      self.cancel_link();
    }
  }

  /// Candidate requirement targets matching [`Self::link_filter`], capped at
  /// `LINK_PICKER_MAX`. Returns the rows and the total number of matches, so
  /// the panel can say how many it is not showing.
  pub fn candidate_requirements(&self) -> (Vec<(NodeId, String)>, usize) {
    let Some(id) = self.selected else {
      return (Vec::new(), 0);
    };
    let store = self.lock();
    let graph = store.graph();
    let existing: HashSet<NodeId> =
      graph.requirements_of(id).map(|e| e.to).collect();
    let today = self.today();
    let mut scored: Vec<(u32, String, NodeId)> = graph
      .nodes()
      .filter(|n| n.id != id && !existing.contains(&n.id))
      .filter_map(|n| {
        let name = describe::node_name(graph, n, today);
        query::score(&self.link_filter, &name).map(|sc| (sc, name, n.id))
      })
      .collect();
    // Best match first; the name, then the id, keep the order stable.
    scored.sort();
    let total = scored.len();
    let v = scored
      .into_iter()
      .take(LINK_PICKER_MAX)
      .map(|(_, name, id)| (id, name))
      .collect();
    (v, total)
  }

  /// Formulas [`Self::link_filter`] reads as, for the requirement search,
  /// leaving out any the selection already requires.
  pub fn atom_offers(&self) -> Vec<AtomOffer> {
    let Some(id) = self.selected else {
      return Vec::new();
    };
    let store = self.lock();
    let graph = store.graph();
    phrase::offers(&self.link_filter, graph, self.today())
      .into_iter()
      .filter(|o| !graph.requirements_of(id).any(|e| e.to == o.atom.node_id()))
      .map(|offer| AtomOffer {
        used_by: graph.dependents_of(offer.atom.node_id()).count(),
        offer,
      })
      .collect()
  }

  /// Make the selection require `offer`'s atom, defining whatever it names
  /// that does not exist yet, as one undo step. An atom some node already
  /// requires is linked to, not added again.
  pub fn require_offer(&mut self, offer: Offer) {
    let Some(id) = self.selected else { return };
    // The requirement first, so the undo step is named for it; an atom may
    // name a referent defined later in the same group.
    let mut events = offer.atom.require(self.lock().graph(), id, EdgeId::new());
    events.extend(offer.define);
    self.commit(events);
  }

  /// Add a dependency requirement from the selected node to `target`.
  pub fn add_requirement(&mut self, target: NodeId) {
    let Some(id) = self.selected else { return };
    self.add_edge(id, target, EdgeKind::Dependency);
  }
}
