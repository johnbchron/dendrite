//! The event log's payloads, the reducer, and inverse generation for undo
//! (PLAN §3).
//!
//! Every UI mutation is one [`Event`]. Events are the source of truth; the
//! [`Graph`] is a projection produced by folding [`Event::apply`] over the
//! log. Undo never deletes history — it appends the *inverse* event, so the
//! log stays monotonic and doubles as an audit trail.

use serde::{Deserialize, Serialize};

use crate::{
  formula::Amount,
  graph::Graph,
  ids::{ContextId, EdgeId, NodeId, PlaceId, QuestId, ResourceId, ScheduleId},
  model::{Edge, EdgeKind, Node, NodeKind, Quest},
  referent::{Context, Place, Resource, Schedule, Span, Unit},
};

/// A single, self-describing mutation of the graph.
///
/// Serde tags every variant with a `"type"` field so a stored payload is
/// self-describing (PLAN §3).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Event {
  /// Add a node to the global namespace.
  NodeAdded {
    /// New node id.
    node:       NodeId,
    /// Task-or-condition payload (carries the initial done bit).
    kind:       NodeKind,
    /// Display name.
    name:       String,
    /// Initial within-level ordering hint.
    order_hint: f64,
  },
  /// Remove a node together with its incident edges and quest claims.
  NodeRemoved {
    /// Node to remove.
    node: NodeId,
  },
  /// Change a node's display name.
  NodeRenamed {
    /// Target node.
    node: NodeId,
    /// New name.
    name: String,
  },
  /// Change a node's within-level ordering hint.
  OrderHintChanged {
    /// Target node.
    node:       NodeId,
    /// New hint.
    order_hint: f64,
  },
  /// Set a task's completion bit.
  TaskCompleted {
    /// Target task node.
    node:      NodeId,
    /// Whether it is now completed.
    completed: bool,
  },
  /// Set a condition's satisfaction bit.
  ConditionSet {
    /// Target condition node.
    node:      NodeId,
    /// Whether it is now satisfied.
    satisfied: bool,
  },
  /// Add a first-class edge (`from` requires `to`).
  EdgeAdded {
    /// New edge id.
    edge: EdgeId,
    /// The edge's kind (only [`EdgeKind::Dependency`] exists).
    kind: EdgeKind,
    /// The dependent.
    from: NodeId,
    /// The requirement.
    to:   NodeId,
  },
  /// Remove an edge.
  EdgeRemoved {
    /// Edge to remove.
    edge: EdgeId,
  },
  /// Create a quest.
  QuestCreated {
    /// New quest id.
    quest: QuestId,
    /// Display name.
    name:  String,
  },
  /// Remove a quest (its claimed nodes are untouched).
  QuestRemoved {
    /// Quest to remove.
    quest: QuestId,
  },
  /// Change a quest's display name.
  QuestRenamed {
    /// Target quest.
    quest: QuestId,
    /// New name.
    name:  String,
  },
  /// Claim a node for a quest.
  QuestClaimed {
    /// Quest gaining the claim.
    quest: QuestId,
    /// Node being claimed.
    node:  NodeId,
  },
  /// Release a quest's claim on a node.
  QuestUnclaimed {
    /// Quest losing the claim.
    quest: QuestId,
    /// Node being released.
    node:  NodeId,
  },
  /// Define a place, or redefine one wholesale.
  PlaceDefined {
    /// New place id.
    place:  PlaceId,
    /// Display name.
    name:   String,
    /// The place it lies in.
    within: Option<PlaceId>,
  },
  /// Edit a place's name and parent.
  PlaceChanged {
    /// Target place.
    place:  PlaceId,
    /// New name.
    name:   String,
    /// New parent.
    within: Option<PlaceId>,
  },
  /// Remove a place. Atoms that point at it stop holding.
  PlaceRemoved {
    /// Place to remove.
    place: PlaceId,
  },
  /// Define a resource, or redefine one wholesale.
  ResourceDefined {
    /// New resource id.
    resource: ResourceId,
    /// Display name.
    name:     String,
    /// What its amounts count.
    unit:     Unit,
    /// How much I have.
    balance:  Amount,
  },
  /// Declare how much of a resource I have.
  ResourceBalanceSet {
    /// Target resource.
    resource: ResourceId,
    /// New balance.
    balance:  Amount,
  },
  /// Change a resource's display name.
  ResourceRenamed {
    /// Target resource.
    resource: ResourceId,
    /// New name.
    name:     String,
  },
  /// Remove a resource. Atoms that point at it stop holding.
  ResourceRemoved {
    /// Resource to remove.
    resource: ResourceId,
  },
  /// Define a schedule, or redefine one wholesale.
  ScheduleDefined {
    /// New schedule id.
    schedule: ScheduleId,
    /// Display name.
    name:     String,
    /// Its windows.
    spans:    Vec<Span>,
  },
  /// Edit a schedule's name and windows.
  ScheduleChanged {
    /// Target schedule.
    schedule: ScheduleId,
    /// New name.
    name:     String,
    /// New windows.
    spans:    Vec<Span>,
  },
  /// Remove a schedule. Atoms that point at it stop holding.
  ScheduleRemoved {
    /// Schedule to remove.
    schedule: ScheduleId,
  },
  /// Define a context, or redefine one wholesale.
  ContextDefined {
    /// New context id.
    context: ContextId,
    /// Display name.
    name:    String,
  },
  /// Change a context's display name.
  ContextRenamed {
    /// Target context.
    context: ContextId,
    /// New name.
    name:    String,
  },
  /// Remove a context. Atoms that point at it stop holding.
  ContextRemoved {
    /// Context to remove.
    context: ContextId,
  },
}

impl Event {
  /// A short lower-case verb phrase for what this event does, for naming an
  /// undo step ("Undo rename").
  pub fn describe(&self) -> &'static str {
    match self {
      Event::NodeAdded {
        kind: NodeKind::Task { .. },
        ..
      } => "add task",
      Event::NodeAdded {
        kind: NodeKind::Condition { .. },
        ..
      } => "add condition",
      Event::NodeRemoved { .. } => "delete",
      Event::NodeRenamed { .. } => "rename",
      Event::OrderHintChanged { .. } => "reorder",
      Event::TaskCompleted {
        completed: true, ..
      } => "complete",
      Event::TaskCompleted { .. } => "reopen",
      Event::ConditionSet {
        satisfied: true, ..
      } => "satisfy",
      Event::ConditionSet { .. } => "unsatisfy",
      Event::EdgeAdded { .. } => "add requirement",
      Event::EdgeRemoved { .. } => "remove requirement",
      Event::QuestCreated { .. } => "new quest",
      Event::QuestRemoved { .. } => "delete quest",
      Event::QuestRenamed { .. } => "rename quest",
      Event::QuestClaimed { .. } => "claim",
      Event::QuestUnclaimed { .. } => "unclaim",
      Event::PlaceDefined { .. } => "new place",
      Event::PlaceChanged { .. } => "edit place",
      Event::PlaceRemoved { .. } => "delete place",
      Event::ResourceDefined { .. } => "new resource",
      Event::ResourceBalanceSet { .. } => "set balance",
      Event::ResourceRenamed { .. } => "rename resource",
      Event::ResourceRemoved { .. } => "delete resource",
      Event::ScheduleDefined { .. } => "new schedule",
      Event::ScheduleChanged { .. } => "edit schedule",
      Event::ScheduleRemoved { .. } => "delete schedule",
      Event::ContextDefined { .. } => "new context",
      Event::ContextRenamed { .. } => "rename context",
      Event::ContextRemoved { .. } => "delete context",
    }
  }

  /// Whether applying `self` right after `earlier` leaves the graph exactly
  /// as applying `self` alone would: both overwrite the same single field of
  /// the same target, so `earlier` has no lasting effect.
  ///
  /// This lets a live edit (a rename typed one keystroke at a time) replace
  /// its previous event in the log rather than pile up one per character.
  pub fn supersedes(&self, earlier: &Event) -> bool {
    match (self, earlier) {
      (
        Event::NodeRenamed { node: a, .. },
        Event::NodeRenamed { node: b, .. },
      ) => a == b,
      (
        Event::QuestRenamed { quest: a, .. },
        Event::QuestRenamed { quest: b, .. },
      ) => a == b,
      (
        Event::OrderHintChanged { node: a, .. },
        Event::OrderHintChanged { node: b, .. },
      ) => a == b,
      (
        Event::PlaceChanged { place: a, .. },
        Event::PlaceChanged { place: b, .. },
      ) => a == b,
      (
        Event::ResourceBalanceSet { resource: a, .. },
        Event::ResourceBalanceSet { resource: b, .. },
      )
      | (
        Event::ResourceRenamed { resource: a, .. },
        Event::ResourceRenamed { resource: b, .. },
      ) => a == b,
      (
        Event::ScheduleChanged { schedule: a, .. },
        Event::ScheduleChanged { schedule: b, .. },
      ) => a == b,
      (
        Event::ContextRenamed { context: a, .. },
        Event::ContextRenamed { context: b, .. },
      ) => a == b,
      _ => false,
    }
  }

  /// Fold this event into `graph`, mutating it in place.
  ///
  /// Application is total and best-effort: an event that targets a missing
  /// entity is a no-op rather than an error, which keeps log replay robust.
  ///
  /// Adding a formula condition that already exists is a no-op too, so two
  /// replicas, or an undo and a redo, adding the same atom agree.
  pub fn apply(&self, graph: &mut Graph) {
    match self {
      Event::NodeAdded {
        node,
        kind,
        name,
        order_hint,
      } => {
        if graph.is_formula(*node) {
          return;
        }
        let mut kind = kind.clone();
        if kind.atom().is_some() {
          // The atom decides; the stored bit stays false.
          if let NodeKind::Condition { satisfied, .. } = &mut kind {
            *satisfied = false;
          }
        }
        graph.insert_node(Node::new(*node, name.clone(), kind, *order_hint));
      }
      Event::NodeRemoved { node } => {
        graph.remove_node(*node);
      }
      Event::NodeRenamed { node, name } => {
        graph.rename_node(*node, name.clone());
      }
      Event::OrderHintChanged { node, order_hint } => {
        graph.set_order_hint(*node, *order_hint);
      }
      Event::TaskCompleted { node, completed } => {
        graph.set_satisfied(*node, *completed);
      }
      Event::ConditionSet { node, satisfied } => {
        graph.set_satisfied(*node, *satisfied);
      }
      Event::EdgeAdded {
        edge,
        kind,
        from,
        to,
      } => {
        graph.insert_edge(Edge::new(*edge, *kind, *from, *to));
      }
      Event::EdgeRemoved { edge } => {
        graph.remove_edge(*edge);
      }
      Event::QuestCreated { quest, name } => {
        graph.insert_quest(Quest::new(*quest, name.clone()));
      }
      Event::QuestRemoved { quest } => {
        graph.remove_quest(*quest);
      }
      Event::QuestRenamed { quest, name } => {
        graph.rename_quest(*quest, name.clone());
      }
      Event::QuestClaimed { quest, node } => {
        graph.claim(*quest, *node);
      }
      Event::QuestUnclaimed { quest, node } => {
        graph.unclaim(*quest, *node);
      }
      Event::PlaceDefined {
        place,
        name,
        within,
      } => {
        graph.insert_place(Place {
          id:     *place,
          name:   name.clone(),
          within: *within,
        });
      }
      Event::PlaceChanged {
        place,
        name,
        within,
      } => {
        if graph.place(*place).is_some() {
          graph.insert_place(Place {
            id:     *place,
            name:   name.clone(),
            within: *within,
          });
        }
      }
      Event::PlaceRemoved { place } => {
        graph.remove_place(*place);
      }
      Event::ResourceDefined {
        resource,
        name,
        unit,
        balance,
      } => {
        graph.insert_resource(Resource {
          id:      *resource,
          name:    name.clone(),
          unit:    unit.clone(),
          balance: *balance,
        });
      }
      Event::ResourceBalanceSet { resource, balance } => {
        if let Some(r) = graph.resource_mut(*resource) {
          r.balance = *balance;
        }
      }
      Event::ResourceRenamed { resource, name } => {
        if let Some(r) = graph.resource_mut(*resource) {
          r.name = name.clone();
        }
      }
      Event::ResourceRemoved { resource } => {
        graph.remove_resource(*resource);
      }
      Event::ScheduleDefined {
        schedule,
        name,
        spans,
      } => {
        graph.insert_schedule(Schedule {
          id:    *schedule,
          name:  name.clone(),
          spans: spans.clone(),
        });
      }
      Event::ScheduleChanged {
        schedule,
        name,
        spans,
      } => {
        if graph.schedule(*schedule).is_some() {
          graph.insert_schedule(Schedule {
            id:    *schedule,
            name:  name.clone(),
            spans: spans.clone(),
          });
        }
      }
      Event::ScheduleRemoved { schedule } => {
        graph.remove_schedule(*schedule);
      }
      Event::ContextDefined { context, name } => {
        graph.insert_context(Context {
          id:   *context,
          name: name.clone(),
        });
      }
      Event::ContextRenamed { context, name } => {
        if let Some(c) = graph.context_mut(*context) {
          c.name = name.clone();
        }
      }
      Event::ContextRemoved { context } => {
        graph.remove_context(*context);
      }
    }
  }

  /// Produce the event(s) that undo this one, computed against the
  /// *pre-state* `graph` (the graph as it was *before* `self` was applied).
  ///
  /// A removal expands into a compound inverse that restores the node/quest
  /// plus every incident edge and claim, so undo is complete. Returns an
  /// empty vec when there is nothing to invert (e.g. a no-op event).
  pub fn inverse(&self, graph: &Graph) -> Vec<Event> {
    match self {
      // Re-adding a formula condition changes nothing, so undoing it must
      // not remove the node the earlier add made.
      Event::NodeAdded { node, .. } if graph.is_formula(*node) => vec![],
      Event::NodeAdded { node, .. } => {
        vec![Event::NodeRemoved { node: *node }]
      }
      Event::NodeRemoved { node } => {
        let Some(n) = graph.node(*node) else {
          return vec![];
        };
        let mut inv = vec![Event::NodeAdded {
          node:       n.id,
          kind:       n.kind.clone(),
          name:       n.name.clone(),
          order_hint: n.order_hint,
        }];
        // Restore incident edges (both directions), de-duplicated by id.
        let mut seen = std::collections::HashSet::new();
        for edge in graph
          .requirements_of(*node)
          .chain(graph.dependents_of(*node))
        {
          if seen.insert(edge.id) {
            inv.push(Event::EdgeAdded {
              edge: edge.id,
              kind: edge.kind,
              from: edge.from,
              to:   edge.to,
            });
          }
        }
        // Restore quest claims.
        for quest in graph.quests() {
          if quest.claims.contains(node) {
            inv.push(Event::QuestClaimed {
              quest: quest.id,
              node:  *node,
            });
          }
        }
        inv
      }
      Event::NodeRenamed { node, .. } => graph
        .node(*node)
        .map(|n| Event::NodeRenamed {
          node: *node,
          name: n.name.clone(),
        })
        .into_iter()
        .collect(),
      Event::OrderHintChanged { node, .. } => graph
        .node(*node)
        .map(|n| Event::OrderHintChanged {
          node:       *node,
          order_hint: n.order_hint,
        })
        .into_iter()
        .collect(),
      Event::TaskCompleted { node, .. } => graph
        .node(*node)
        .map(|n| Event::TaskCompleted {
          node:      *node,
          completed: n.kind.is_satisfied(),
        })
        .into_iter()
        .collect(),
      Event::ConditionSet { node, .. } => graph
        .node(*node)
        .filter(|n| n.kind.atom().is_none())
        .map(|n| Event::ConditionSet {
          node:      *node,
          satisfied: n.kind.is_satisfied(),
        })
        .into_iter()
        .collect(),
      Event::EdgeAdded { edge, .. } => {
        vec![Event::EdgeRemoved { edge: *edge }]
      }
      Event::EdgeRemoved { edge } => graph
        .edge(*edge)
        .map(|e| Event::EdgeAdded {
          edge: e.id,
          kind: e.kind,
          from: e.from,
          to:   e.to,
        })
        .into_iter()
        .collect(),
      Event::QuestCreated { quest, .. } => {
        vec![Event::QuestRemoved { quest: *quest }]
      }
      Event::QuestRemoved { quest } => {
        let Some(q) = graph.quest(*quest) else {
          return vec![];
        };
        let mut inv = vec![Event::QuestCreated {
          quest: q.id,
          name:  q.name.clone(),
        }];
        for node in &q.claims {
          inv.push(Event::QuestClaimed {
            quest: *quest,
            node:  *node,
          });
        }
        inv
      }
      Event::QuestRenamed { quest, .. } => graph
        .quest(*quest)
        .map(|q| Event::QuestRenamed {
          quest: *quest,
          name:  q.name.clone(),
        })
        .into_iter()
        .collect(),
      Event::QuestClaimed { quest, node } => {
        vec![Event::QuestUnclaimed {
          quest: *quest,
          node:  *node,
        }]
      }
      Event::QuestUnclaimed { quest, node } => {
        vec![Event::QuestClaimed {
          quest: *quest,
          node:  *node,
        }]
      }
      // A definition may replace an existing one; undo restores it.
      Event::PlaceDefined { place, .. } => vec![
        restore_place(graph, *place)
          .unwrap_or(Event::PlaceRemoved { place: *place }),
      ],
      Event::PlaceChanged { place, .. } | Event::PlaceRemoved { place } => {
        restore_place(graph, *place).into_iter().collect()
      }
      Event::ResourceDefined { resource, .. } => {
        vec![restore_resource(graph, *resource).unwrap_or(
          Event::ResourceRemoved {
            resource: *resource,
          },
        )]
      }
      Event::ResourceBalanceSet { resource, .. } => graph
        .resource(*resource)
        .map(|r| Event::ResourceBalanceSet {
          resource: *resource,
          balance:  r.balance,
        })
        .into_iter()
        .collect(),
      Event::ResourceRenamed { resource, .. } => graph
        .resource(*resource)
        .map(|r| Event::ResourceRenamed {
          resource: *resource,
          name:     r.name.clone(),
        })
        .into_iter()
        .collect(),
      Event::ResourceRemoved { resource } => {
        restore_resource(graph, *resource).into_iter().collect()
      }
      Event::ScheduleDefined { schedule, .. } => {
        vec![restore_schedule(graph, *schedule).unwrap_or(
          Event::ScheduleRemoved {
            schedule: *schedule,
          },
        )]
      }
      Event::ScheduleChanged { schedule, .. } => graph
        .schedule(*schedule)
        .map(|s| Event::ScheduleChanged {
          schedule: *schedule,
          name:     s.name.clone(),
          spans:    s.spans.clone(),
        })
        .into_iter()
        .collect(),
      Event::ScheduleRemoved { schedule } => {
        restore_schedule(graph, *schedule).into_iter().collect()
      }
      Event::ContextDefined { context, .. } => {
        vec![
          restore_context(graph, *context)
            .unwrap_or(Event::ContextRemoved { context: *context }),
        ]
      }
      Event::ContextRenamed { context, .. } => graph
        .context(*context)
        .map(|c| Event::ContextRenamed {
          context: *context,
          name:    c.name.clone(),
        })
        .into_iter()
        .collect(),
      Event::ContextRemoved { context } => {
        restore_context(graph, *context).into_iter().collect()
      }
    }
  }
}

/// The event that redefines `place` as `graph` holds it.
fn restore_place(graph: &Graph, place: PlaceId) -> Option<Event> {
  graph.place(place).map(|p| Event::PlaceDefined {
    place:  p.id,
    name:   p.name.clone(),
    within: p.within,
  })
}

/// The event that redefines `resource` as `graph` holds it.
fn restore_resource(graph: &Graph, resource: ResourceId) -> Option<Event> {
  graph.resource(resource).map(|r| Event::ResourceDefined {
    resource: r.id,
    name:     r.name.clone(),
    unit:     r.unit.clone(),
    balance:  r.balance,
  })
}

/// The event that redefines `schedule` as `graph` holds it.
fn restore_schedule(graph: &Graph, schedule: ScheduleId) -> Option<Event> {
  graph.schedule(schedule).map(|s| Event::ScheduleDefined {
    schedule: s.id,
    name:     s.name.clone(),
    spans:    s.spans.clone(),
  })
}

/// The event that redefines `context` as `graph` holds it.
fn restore_context(graph: &Graph, context: ContextId) -> Option<Event> {
  graph.context(context).map(|c| Event::ContextDefined {
    context: c.id,
    name:    c.name.clone(),
  })
}

/// Apply a batch of events in order, returning the batch that undoes it.
///
/// The returned inverse is ordered so that replaying it exactly reverses the
/// batch: each event's inverse is captured against the state *before* that
/// event, then the inverses are reversed as a whole.
pub fn apply_batch(graph: &mut Graph, events: &[Event]) -> Vec<Event> {
  let mut inverse = Vec::new();
  for event in events {
    let mut inv = event.inverse(graph);
    event.apply(graph);
    // Prepend (reverse order) so undo unwinds last-applied first.
    inv.reverse();
    inverse.extend(inv);
  }
  inverse.reverse();
  inverse
}

/// Apply one user gesture's events as [`apply_batch`] does, then remove
/// every formula condition the gesture left with no dependents, returning
/// the events actually applied (the batch, then those removals) and the
/// batch that undoes them all.
///
/// A formula node exists only to be required, so the gesture that removes
/// its last requirement removes it too, in the same undo group; undo brings
/// both back through the ordinary inverses. The removals are real events in
/// the log, so replay needs no garbage collection of its own.
pub fn apply_group(
  graph: &mut Graph,
  mut events: Vec<Event>,
) -> (Vec<Event>, Vec<Event>) {
  // The formula nodes that lost a requirement, each seen against the state
  // just before the event that unlinked it.
  let mut unlinked = Vec::new();
  let mut inverse = Vec::new();
  for event in &events {
    unlinked.extend(unlinks(event, graph));
    let mut inv = event.inverse(graph);
    event.apply(graph);
    inv.reverse();
    inverse.extend(inv);
  }
  unlinked.sort_unstable();
  unlinked.dedup();
  for node in unlinked {
    if graph.is_formula(node) && graph.dependents_of(node).next().is_none() {
      let removal = Event::NodeRemoved { node };
      let mut inv = removal.inverse(graph);
      removal.apply(graph);
      inv.reverse();
      inverse.extend(inv);
      events.push(removal);
    }
  }
  inverse.reverse();
  (events, inverse)
}

/// The formula conditions `event` would take a requirement away from, read
/// against the state before it: the target of a removed edge, or the
/// formula requirements of a removed node.
fn unlinks(event: &Event, graph: &Graph) -> Vec<NodeId> {
  let targets: Vec<NodeId> = match event {
    Event::EdgeRemoved { edge } => {
      graph.edge(*edge).map(|e| e.to).into_iter().collect()
    }
    Event::NodeRemoved { node } => {
      graph.requirements_of(*node).map(|e| e.to).collect()
    }
    _ => vec![],
  };
  targets
    .into_iter()
    .filter(|n| graph.is_formula(*n))
    .collect()
}
