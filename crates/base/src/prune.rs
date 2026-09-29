//! Pruning what formula conditions leave behind: formula nodes nothing
//! requires, and referents no atom points at any more.
//!
//! Referents outlive the atoms that introduced them — typing "at the
//! hardware store" once defines the place for good — so they pile up in
//! the pickers. [`prune`] names what can go as ordinary removal events, so
//! committing them is one undoable step and nothing is collected behind the
//! log's back.

use std::collections::BTreeSet;

use crate::{
  Atom, ContextId, Event, Facts, Graph, NodeId, PlaceId, ResourceId, ScheduleId,
};

/// The removals that prune `graph`, in order: every formula condition with
/// no dependents, then every referent no remaining atom uses.
///
/// What `facts` holds right now is kept: the places I am at and the active
/// contexts, so pruning never pulls the ground out from under the context
/// bar. A place stays too if it lies within a place in use (so a group
/// like "Errands" keeps its members) or a place in use lies within it (so
/// a kept place keeps its parents).
pub fn prune(graph: &Graph, facts: &Facts) -> Vec<Event> {
  let orphans: BTreeSet<NodeId> = graph
    .nodes()
    .filter(|n| {
      n.kind.atom().is_some() && graph.dependents_of(n.id).next().is_none()
    })
    .map(|n| n.id)
    .collect();

  let mut places: BTreeSet<PlaceId> = facts.places.iter().copied().collect();
  let mut resources = BTreeSet::<ResourceId>::new();
  let mut schedules = BTreeSet::<ScheduleId>::new();
  let mut contexts: BTreeSet<ContextId> =
    facts.contexts.iter().copied().collect();
  for node in graph.nodes().filter(|n| !orphans.contains(&n.id)) {
    match node.kind.atom() {
      Some(Atom::At { place }) => {
        places.insert(*place);
      }
      Some(Atom::Has { resource, .. }) => {
        resources.insert(*resource);
      }
      Some(Atom::Within { schedule }) => {
        schedules.insert(*schedule);
      }
      Some(Atom::In { context }) => {
        contexts.insert(*context);
      }
      Some(Atom::After { .. } | Atom::Before { .. } | Atom::Free { .. })
      | None => {}
    }
  }
  let place_kept = |p: PlaceId| {
    places
      .iter()
      .any(|&u| graph.is_within(p, u) || graph.is_within(u, p))
  };

  let mut events: Vec<Event> = orphans
    .into_iter()
    .map(|node| Event::NodeRemoved { node })
    .collect();
  // BTreeSets, so the removals come out in id order whatever the maps do.
  events.extend(
    graph
      .places()
      .map(|p| p.id)
      .collect::<BTreeSet<_>>()
      .into_iter()
      .filter(|&p| !place_kept(p))
      .map(|place| Event::PlaceRemoved { place }),
  );
  events.extend(
    graph
      .resources()
      .map(|r| r.id)
      .collect::<BTreeSet<_>>()
      .difference(&resources)
      .map(|&resource| Event::ResourceRemoved { resource }),
  );
  events.extend(
    graph
      .schedules()
      .map(|s| s.id)
      .collect::<BTreeSet<_>>()
      .difference(&schedules)
      .map(|&schedule| Event::ScheduleRemoved { schedule }),
  );
  events.extend(
    graph
      .contexts()
      .map(|c| c.id)
      .collect::<BTreeSet<_>>()
      .difference(&contexts)
      .map(|&context| Event::ContextRemoved { context }),
  );
  events
}
