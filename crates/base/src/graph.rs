//! The global graph container and its structural mutations (PLAN §1, §2).
//!
//! The graph is a plain in-memory projection: maps of nodes, edges and
//! quests plus cached adjacency indices. All derived state (readiness,
//! cycles, quest scope) is computed on demand from this structure and never
//! stored (PLAN §2 "Derived state").

use std::collections::{HashMap, HashSet};

use crate::{
  ids::{ContextId, EdgeId, NodeId, PlaceId, QuestId, ResourceId, ScheduleId},
  model::{Edge, Node, Quest},
  referent::{Context, Place, Resource, Schedule},
};

/// The whole world: one flat, global, fully cross-linkable graph, the
/// quests that claim into it, and the referents formula atoms point at.
#[derive(Clone, Debug, Default)]
pub struct Graph {
  nodes:     HashMap<NodeId, Node>,
  edges:     HashMap<EdgeId, Edge>,
  quests:    HashMap<QuestId, Quest>,
  places:    HashMap<PlaceId, Place>,
  resources: HashMap<ResourceId, Resource>,
  schedules: HashMap<ScheduleId, Schedule>,
  contexts:  HashMap<ContextId, Context>,
  /// `from` node -> edges leaving it (its requirements).
  out:       HashMap<NodeId, Vec<EdgeId>>,
  /// `to` node -> edges arriving at it (its dependents).
  inc:       HashMap<NodeId, Vec<EdgeId>>,
}

/// Two graphs are equal when they hold the same nodes, edges, quests and
/// referents. Adjacency indices are derived and deliberately excluded, so a
/// graph that reached a state by different mutation paths still compares
/// equal.
impl PartialEq for Graph {
  fn eq(&self, other: &Self) -> bool {
    self.nodes == other.nodes
      && self.edges == other.edges
      && self.quests == other.quests
      && self.places == other.places
      && self.resources == other.resources
      && self.schedules == other.schedules
      && self.contexts == other.contexts
  }
}

impl Graph {
  /// An empty graph.
  pub fn new() -> Self { Self::default() }

  // --- read access -------------------------------------------------------

  /// Look a node up by id.
  pub fn node(&self, id: NodeId) -> Option<&Node> { self.nodes.get(&id) }

  /// Look an edge up by id.
  pub fn edge(&self, id: EdgeId) -> Option<&Edge> { self.edges.get(&id) }

  /// Look a quest up by id.
  pub fn quest(&self, id: QuestId) -> Option<&Quest> { self.quests.get(&id) }

  /// Iterate all nodes in arbitrary order.
  pub fn nodes(&self) -> impl Iterator<Item = &Node> { self.nodes.values() }

  /// Iterate all edges in arbitrary order.
  pub fn edges(&self) -> impl Iterator<Item = &Edge> { self.edges.values() }

  /// Iterate all quests in arbitrary order.
  pub fn quests(&self) -> impl Iterator<Item = &Quest> { self.quests.values() }

  /// Number of nodes.
  pub fn node_count(&self) -> usize { self.nodes.len() }

  /// The edges leaving `node` — the things it requires (`from == node`).
  pub fn requirements_of(&self, node: NodeId) -> impl Iterator<Item = &Edge> {
    self
      .out
      .get(&node)
      .into_iter()
      .flatten()
      .filter_map(|e| self.edges.get(e))
  }

  /// The edges arriving at `node` — the things that depend on it
  /// (`to == node`).
  pub fn dependents_of(&self, node: NodeId) -> impl Iterator<Item = &Edge> {
    self
      .inc
      .get(&node)
      .into_iter()
      .flatten()
      .filter_map(|e| self.edges.get(e))
  }

  /// The subgraph on the nodes `keep` accepts: those nodes and the edges
  /// between them. Quests are left out, since a subgraph is for drawing,
  /// not for asking what claims what.
  pub fn induced(&self, keep: impl Fn(NodeId) -> bool) -> Graph {
    let mut sub = Graph::new();
    for node in self.nodes.values().filter(|n| keep(n.id)) {
      sub.insert_node(node.clone());
    }
    for edge in self.edges.values().filter(|e| keep(e.from) && keep(e.to)) {
      sub.insert_edge(*edge);
    }
    sub
  }

  /// Whether `node`'s stored bit is set (completed task / satisfied manual
  /// condition). Formula conditions depend on facts; gating reads
  /// [`Derived::is_satisfied`](crate::Derived::is_satisfied).
  pub fn is_satisfied(&self, node: NodeId) -> bool {
    self.nodes.get(&node).is_some_and(|n| n.kind.is_satisfied())
  }

  /// Whether `node` is a formula condition. Formula conditions are sinks:
  /// derivation ignores any edge leaving one.
  pub fn is_formula(&self, node: NodeId) -> bool {
    self
      .nodes
      .get(&node)
      .is_some_and(|n| n.kind.atom().is_some())
  }

  /// Look a place up by id.
  pub fn place(&self, id: PlaceId) -> Option<&Place> { self.places.get(&id) }

  /// Look a resource up by id.
  pub fn resource(&self, id: ResourceId) -> Option<&Resource> {
    self.resources.get(&id)
  }

  /// Look a schedule up by id.
  pub fn schedule(&self, id: ScheduleId) -> Option<&Schedule> {
    self.schedules.get(&id)
  }

  /// Look a context up by id.
  pub fn context(&self, id: ContextId) -> Option<&Context> {
    self.contexts.get(&id)
  }

  /// Iterate all places in arbitrary order.
  pub fn places(&self) -> impl Iterator<Item = &Place> { self.places.values() }

  /// Iterate all resources in arbitrary order.
  pub fn resources(&self) -> impl Iterator<Item = &Resource> {
    self.resources.values()
  }

  /// Iterate all schedules in arbitrary order.
  pub fn schedules(&self) -> impl Iterator<Item = &Schedule> {
    self.schedules.values()
  }

  /// Iterate all contexts in arbitrary order.
  pub fn contexts(&self) -> impl Iterator<Item = &Context> {
    self.contexts.values()
  }

  /// Whether being at `place` means being at `outer`: they are the same
  /// place, or `outer` is reached by following `within` up from `place`.
  /// A `within` loop is walked once round and no further.
  pub fn is_within(&self, place: PlaceId, outer: PlaceId) -> bool {
    let mut at = Some(place);
    for _ in 0..=self.places.len() {
      match at {
        Some(p) if p == outer => return true,
        Some(p) => at = self.places.get(&p).and_then(|p| p.within),
        None => return false,
      }
    }
    false
  }

  // --- structural mutations ---------------------------------------------
  //
  // These are the primitives the event reducer drives. They keep the
  // adjacency indices consistent and never leave dangling references.

  /// Insert or replace a node. Adjacency is unaffected (edges are separate).
  pub fn insert_node(&mut self, node: Node) {
    self.out.entry(node.id).or_default();
    self.inc.entry(node.id).or_default();
    self.nodes.insert(node.id, node);
  }

  /// Remove a node together with every incident edge and every quest claim
  /// on it, so no dangling reference survives. Returns the removed node, the
  /// removed edges, and the quests that had claimed it — enough to build a
  /// complete inverse for undo.
  pub fn remove_node(
    &mut self,
    id: NodeId,
  ) -> Option<(Node, Vec<Edge>, Vec<QuestId>)> {
    let node = self.nodes.remove(&id)?;

    let incident: Vec<EdgeId> = self
      .out
      .get(&id)
      .into_iter()
      .chain(self.inc.get(&id))
      .flatten()
      .copied()
      .collect();
    let mut removed_edges = Vec::new();
    for e in incident {
      if let Some(edge) = self.remove_edge(e) {
        removed_edges.push(edge);
      }
    }

    let mut unclaimed = Vec::new();
    for quest in self.quests.values_mut() {
      if quest.claims.remove(&id) {
        unclaimed.push(quest.id);
      }
    }

    self.out.remove(&id);
    self.inc.remove(&id);
    Some((node, removed_edges, unclaimed))
  }

  /// Rename a node, returning the previous name.
  pub fn rename_node(&mut self, id: NodeId, name: String) -> Option<String> {
    self
      .nodes
      .get_mut(&id)
      .map(|n| core::mem::replace(&mut n.name, name))
  }

  /// Set a node's order hint, returning the previous value.
  pub fn set_order_hint(&mut self, id: NodeId, hint: f64) -> Option<f64> {
    self
      .nodes
      .get_mut(&id)
      .map(|n| core::mem::replace(&mut n.order_hint, hint))
  }

  /// Set a node's completion/satisfaction bit, returning the previous value.
  pub fn set_satisfied(&mut self, id: NodeId, value: bool) -> Option<bool> {
    self.nodes.get_mut(&id).map(|n| n.kind.set_satisfied(value))
  }

  /// Insert an edge, wiring it into both adjacency indices.
  pub fn insert_edge(&mut self, edge: Edge) {
    self.out.entry(edge.from).or_default().push(edge.id);
    self.inc.entry(edge.to).or_default().push(edge.id);
    self.edges.insert(edge.id, edge);
  }

  /// Remove an edge, returning it if it existed.
  pub fn remove_edge(&mut self, id: EdgeId) -> Option<Edge> {
    let edge = self.edges.remove(&id)?;
    if let Some(v) = self.out.get_mut(&edge.from) {
      v.retain(|e| *e != id);
    }
    if let Some(v) = self.inc.get_mut(&edge.to) {
      v.retain(|e| *e != id);
    }
    Some(edge)
  }

  /// Insert or replace a quest.
  pub fn insert_quest(&mut self, quest: Quest) {
    self.quests.insert(quest.id, quest);
  }

  /// Remove a quest (its claimed nodes are untouched — quests own nothing),
  /// returning it if it existed.
  pub fn remove_quest(&mut self, id: QuestId) -> Option<Quest> {
    self.quests.remove(&id)
  }

  /// Rename a quest, returning the previous name.
  pub fn rename_quest(&mut self, id: QuestId, name: String) -> Option<String> {
    self
      .quests
      .get_mut(&id)
      .map(|q| core::mem::replace(&mut q.name, name))
  }

  /// Claim a node for a quest. Returns `true` if this added a new claim.
  pub fn claim(&mut self, quest: QuestId, node: NodeId) -> bool {
    self
      .quests
      .get_mut(&quest)
      .is_some_and(|q| q.claims.insert(node))
  }

  /// Release a node's claim from a quest. Returns `true` if a claim existed.
  pub fn unclaim(&mut self, quest: QuestId, node: NodeId) -> bool {
    self
      .quests
      .get_mut(&quest)
      .is_some_and(|q| q.claims.remove(&node))
  }

  /// Insert or replace a place, returning the one it replaced.
  pub fn insert_place(&mut self, place: Place) -> Option<Place> {
    self.places.insert(place.id, place)
  }

  /// Remove a place, returning it if it existed. Places within it keep
  /// their now-dangling `within`, which simply leads nowhere.
  pub fn remove_place(&mut self, id: PlaceId) -> Option<Place> {
    self.places.remove(&id)
  }

  /// Insert or replace a resource, returning the one it replaced.
  pub fn insert_resource(&mut self, resource: Resource) -> Option<Resource> {
    self.resources.insert(resource.id, resource)
  }

  /// Remove a resource, returning it if it existed.
  pub fn remove_resource(&mut self, id: ResourceId) -> Option<Resource> {
    self.resources.remove(&id)
  }

  /// A resource, for editing in place.
  pub fn resource_mut(&mut self, id: ResourceId) -> Option<&mut Resource> {
    self.resources.get_mut(&id)
  }

  /// Insert or replace a schedule, returning the one it replaced.
  pub fn insert_schedule(&mut self, schedule: Schedule) -> Option<Schedule> {
    self.schedules.insert(schedule.id, schedule)
  }

  /// Remove a schedule, returning it if it existed.
  pub fn remove_schedule(&mut self, id: ScheduleId) -> Option<Schedule> {
    self.schedules.remove(&id)
  }

  /// Insert or replace a context, returning the one it replaced.
  pub fn insert_context(&mut self, context: Context) -> Option<Context> {
    self.contexts.insert(context.id, context)
  }

  /// Remove a context, returning it if it existed.
  pub fn remove_context(&mut self, id: ContextId) -> Option<Context> {
    self.contexts.remove(&id)
  }

  /// A context, for editing in place.
  pub fn context_mut(&mut self, id: ContextId) -> Option<&mut Context> {
    self.contexts.get_mut(&id)
  }

  /// The distinct targets that gate `node` (deduplicated across
  /// parallel/multi-kind edges). None for a formula condition, which is a
  /// sink whatever edges leave it.
  pub(crate) fn requirement_targets(&self, node: NodeId) -> HashSet<NodeId> {
    if self.is_formula(node) {
      return HashSet::new();
    }
    self.requirements_of(node).map(|e| e.to).collect()
  }
}
