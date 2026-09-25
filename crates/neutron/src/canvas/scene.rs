//! What the app hands the canvas to draw, and what the canvas reports back.

use std::collections::HashSet;

use base::{EdgeId, NodeId, NodeKind, NodeState};
use layout::Arrangement;

/// One node as the canvas needs to draw and hit-test it.
#[derive(Clone, Debug)]
pub struct RenderNode {
  /// Which node this is (returned in [`CanvasAction::Click`]).
  pub id:       NodeId,
  /// Display text.
  pub label:    String,
  /// Task vs. condition — selects the shape.
  pub kind:     NodeKind,
  /// Derived status — selects the fill/border colours.
  pub state:    NodeState,
  /// Whether this node is the current selection.
  pub selected: bool,
  /// Whether this node is only pulled into the active quest's scope (not
  /// claimed) — rendered dimmed (PLAN §5).
  pub dimmed:   bool,
}

/// One edge between two rendered nodes, plus styling flags.
#[derive(Clone, Debug)]
pub struct RenderEdge {
  /// Which edge this is, to find the channels it was given.
  pub id:       EdgeId,
  /// The dependent end (the arrow points here).
  pub from:     NodeId,
  /// The requirement end.
  pub to:       NodeId,
  /// Whether the cycle-cut reversed this edge (a backward cycle edge).
  pub reversed: bool,
}

/// A complete, self-contained description of what to paint.
#[derive(Clone, Debug, Default)]
pub struct CanvasScene {
  /// Nodes, painted on top of edges.
  pub nodes:       Vec<RenderNode>,
  /// Edges, painted underneath.
  pub edges:       Vec<RenderEdge>,
  /// The rows and order the nodes are placed in. Coordinates are assigned
  /// by the widget, which is the only place label sizes are known.
  pub arrangement: Arrangement,
}

/// Link mode, as the canvas shows it: which node is gaining requirements,
/// which nodes a click cannot add, and which would close a cycle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkMode {
  /// The node gaining requirements (the selection).
  pub source:       NodeId,
  /// Its name, for the banner.
  pub name:         String,
  /// The source itself and the nodes it already requires: dimmed, and a
  /// click on them adds nothing.
  pub taken:        HashSet<NodeId>,
  /// Nodes that already require the source, so requiring them would close a
  /// cycle: allowed, but outlined as a warning.
  pub closes_cycle: HashSet<NodeId>,
}

/// Something the user did on the canvas that the app must react to.
#[derive(Clone, Debug)]
pub enum CanvasAction {
  /// A click on a node, or on empty space (`None`), with Shift held or not.
  Click {
    /// The node clicked, if any.
    node:  Option<NodeId>,
    /// Whether Shift was held.
    shift: bool,
  },
  /// The zoom level, as a whole percentage, changed.
  Zoomed(u32),
}
