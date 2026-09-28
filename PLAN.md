# Neutron — Planning Document

Status: **all review questions resolved — awaiting final approval**
Date: 2026-09-08

A dependency-graph task manager built in Rust on the linebender stack. All
tasks live in **one global, flat, fully cross-linkable graph**; quests are
*claims* on that graph (epics), not containers. The primary view is an
auto-laid-out, layered DAG with clear visual states.

---

## 1. Product definition

- Personal, single-user, local-first tool. No sync/multi-user in v1, but the
  data model must not preclude CRDT-style sync later (ULID ids already do this;
  the event log extends it).
- **Global namespace**: all nodes and edges exist in one graph, one local
  database. Readiness, completion, and cycle flags are global state — a node
  completed anywhere is completed everywhere.
- **Quests are lenses, not containers**: a quest *claims* a set of nodes as an
  epic. Claims are many-to-many — a node can belong to several quests or none.
  Claiming mutable gates? No: claiming/unclaiming never affects node state.
- The graph stays **flat**: there is no nesting and no subtask relation. Any
  node can require any other node, with no hierarchy limits.
- Primary view: a pannable/zoomable canvas rendering the graph as a layered
  DAG (Sugiyama-style). A text view exists but the canvas leads.
- Must stay usable at 1k+ nodes: incremental layout, viewport culling, minimap,
  and focus/subgraph filtering are v1 goals, not stretch goals.

## 2. Data model (revised vs. current `base`)

### Nodes

```rust
struct Node {
  id: NodeId,            // ULID (keep)
  name: String,          // free-form display text — lookup is id-only
  kind: NodeKind,        // Task | Condition
  // user-supplied within-level ordering hint for layout
  order_hint: f64,
}

struct Task { completed: bool }
struct Condition { satisfied: bool, source: ConditionSource }
enum ConditionSource {
  Manual,
  // v2: Auto(...) — reserved in schema now, not implemented in v1
}

// Quests claim nodes from the global namespace
struct Quest {
  id: QuestId,
  name: String,
  claims: HashSet<NodeId>,  // the former `roots` field — membership, not ownership
}
```

Changes from the current skeleton:

- Node names are **free-form**, uniqueness dropped: the `BiHashMap` name key
  goes; lookups and text-view references are id-only.
- **Edges become first-class**, moved out of `NodeData::Task.deps`:
  ```rust
  struct Edge {
    id: EdgeId,           // needed for event log and reordering
    kind: EdgeKind,       // Dependency (the only kind)
    from: NodeId,         // the dependent
    to: NodeId,           // the requirement
  }
  ```
  `from → to` always reads "**from** requires **to**".
- `NodeKind` + payload instead of `NodeData` enum, since tasks and conditions
  both hold edges now.
- `ConditionSource` reserved so auto-evaluated conditions slot in without
  schema migration later.

### Derived state (never stored, always computed)

| Node      | State      | Rule                                              |
|-----------|------------|---------------------------------------------------|
| Task      | Completed  | `completed`                                       |
| Task      | Ready      | all incoming-requirement targets satisfied        |
| Task      | Blocked    | ≥1 requirement target unsatisfied                 |
| Task      | Cyclic     | member of a detected cycle → excluded from Ready  |
| Condition | Satisfied  | `satisfied`                                       |
| Condition | Pending    | !`satisfied`                                      |

- Ready requires **ALL** dependency targets satisfied (AND semantics, v1 only
  — no ANY/n-of-N).
- Cycles are **allowed but flagged**: SCC detection (Tarjan) marks members
  Cyclic; they render invalid and are treated as permanently blocked until the
  cycle is broken. Layout uses a feedback-arc heuristic to rank them anyway.

### Quest-scoped queries

Readiness is computed on the **whole global graph**; quests only filter what
is shown. Two core queries per quest:

- **Scope view**: the set of rendered nodes = claimed nodes ∪ the transitive
  requirement closure beneath them. Pulled-in nodes not claimed by the quest
  render visually distinct (see §5), since completing them affects global state.
- **Actionable query**: claim-relevant nodes that are Ready, evaluated
  *all the way down the graph*. Sinks (completed/satisfied chains) drop out;
  what remains is the set of immediately executable frontiers of the epic,
  including pulled-in Ready work claimed by other quests or none.
- `Quest.roots` is **kept, reinterpreted**: it is the claim set (many-to-many
  quest↔node relation). It defines membership/scope but owns nothing; deleting
  a quest never deletes its nodes.

## 3. Storage & event log

SQLite (propose `rusqlite`, WAL mode), **one database for the whole global
graph** (quests live inside it):

- `events` — append-only, the source of truth. Each UI mutation is one event:
  `NodeAdded`, `NodeRenamed`, `TaskCompleted`, `EdgeAdded(kind)`, `EdgeRemoved`,
  `QuestCreated`, `QuestClaimed(node)`, `QuestUnclaimed(node)`,
  `OrderHintChanged`, ... Each event is a self-describing payload (serde JSON
  is fine) + seq + ULID.
- No projection tables. The log is replayed in full on open into the
  in-memory `Graph`, which is the only projection and is what every read goes
  through. (v1 shipped materialized `nodes`/`edges`/`quests`/`quest_claims`
  tables plus a `snapshot_seq`; they were a startup cache nothing else read,
  and schema v2 drops them.)
- Sync-friendliness: the log is the eventual sync substrate. Undo appends an
  inverse event rather than deleting history (history doubles as audit trail).

Undo/redo in v1: in-memory stack of events; undo applies and appends the
inverse event, keeping the log monotonic.

## 4. Architecture

```
crates/
  base/      pure domain model, graph algorithms (deps resolution, Tarjan SCC,
             readiness, quest-scope closure + actionable queries), event types
             + reducer, serde. No I/O, no UI.
  session/   the editing session: graph + undo/redo over an abstract log.
  db/        the SQLite backend for a session: event log, migrations.
  app/       all application behaviour, with no UI toolkit: state, commands,
             the key map, palettes, and the scene a canvas is handed.
  widgets/   generic masonry/xilem widgets, knowing nothing of this app.
  layout/    (new, pure) Sugiyama: cycle cut → ranking → within-level ordering
             (barycenter crossing-minimization seeded by order_hint) →
             x-coordinate assignment. Incremental-friendly API.
  neutron/   xilem app: state, custom canvas widget, panels, wiring.
```

Rationale: `base` and `layout` stay pure and exhaustively testable; `neutron`
holds only glue and painting. The custom canvas widget is a masonry custom
widget (hit-testing + pan/zoom + vello painting) — flagged as the highest
technical risk (see §6).

## 5. UI specification (canvas)

- **Node visuals**: distinct shape per kind (e.g. rounded rect = Task,
  diamond = Condition) so shape, not color alone, carries meaning.
- **State visuals**: Completed (filled/dimmed + check), Ready (accent border,
  "actionable"), Blocked (neutral/grey), Cyclic (distinct warning treatment),
  Condition Pending/Satisfied (empty vs filled diamond).
- **Edges**: every edge is a dependency, drawn solid. Direction arrows point
  at the dependent (from a requirement to what it unblocks).
- **Layout**: fully automatic levelling, **top-down: quest roots/dependents
  at the top, requirements below** (progress flows upward toward the goal;
  backward edges of allowed cycles point upward and are styled as such).
  User cannot freely drag, but **can reorder nodes within a
  level**; that gesture writes `order_hint` events which seed crossing
  minimization so the chosen order survives relayout.
- **Interactions (canvas)**: click select; click "complete" affordance (only
  on Ready/Completed tasks and on conditions); drag node-to-edge-port to
  create edge (choose kind via modifier or popup); delete/backspace removes
  selection; drag within level reorders; wheel/pan navigation; hover shows
  incoming/outgoing reason on blocked nodes ("blocked by X, Y").
- **Side panel**: node properties (name, state explanation, requirements
  list), search/jump, minimap.
- **Text view**: live-synced RON serialization of nodes, edges, and quest
  claims (not the event log); edits parse→validate→apply through the same
  event pipeline as the canvas. Errors surfaced without corrupting state.
- **Focus/collapse**: select node → "focus" collapses to its neighborhood/
  ancestor cone — required for 1k+ node ergonomics.
- **Quest chrome**: a quest switcher (sidebar or dropdown) supports the "all
  nodes" view (no quest selected) plus per-quest scoped views. In a scoped
  view, pulled-in unclaimed nodes render dimmed/outlined with a distinct
  "not in this quest" treatment; multi-claimed nodes can badge their other
  quests on hover. Claim/unclaim is a canvas and side-panel gesture
  (e.g. drag node to quest, toggle in properties).
- **Actionable view**: per quest, an "Actionable" filter/list (and canvas
  highlight mode) running the §2 actionable query — the at-a-glance answer to
  "what can I do right now on this epic?".

## 6. Risks

1. **Masonry custom-canvas widget** — no ready-made canvas in the linebender
   ecosystem; spike early (Milestone 3) before storage work is polished.
2. **Sugiyama on cyclic graphs** — feedback-arc cut is heuristic; acceptable
   imperfection, Cyclic styling communicates it.
3. **Layout stability** — relayout after each edit must not shuffle the
   viewport; order hints + stable rank assignment need care.
4. **Name as secondary key** — resolved by decision in §7, but requires the
   `BiHashMap` rework in `base` to land first (id-only node map).

## 7. Decisions (resolved via review)

1. **Names**: id-only lookup, free-form display names, no uniqueness.
2. **Actionable view**: pulled-in Ready work from outside the quest **is
   listed**, badged with its claiming quest(s).
3. **Levelling**: top-down, roots (dependents/goals) at the top, dependencies
   below.
4. **Crates**: `base` stays pure; SQLite in a separate `db` crate.
5. **Edge kinds**: open-ended enum; v1 ships Dependency only. Subtasks were
   dropped: requiring a node already expresses "this is part of that".

## 8. Milestones

| # | Milestone | Done when |
|---|-----------|-----------|
| 1 | Core model | Edges first-class; quest claims + scope-closure & actionable queries; readiness + Tarjan SCC + cycle flagging with property tests |
| 2 | Persistence | SQLite global DB: event log, undo/redo via inverse events; round-trip tests |
| 3 | Canvas spike | Custom masonry widget renders N nodes, pan/zoom, hit-test, state styling, hardcoded layout |
| 4 | Auto layout | Sugiyama incl. cycle cut + order hints, integrated into canvas; within-level reorder gesture |
| 5 | Canvas editing | Add/rename/complete/edge-create/delete, claim/unclaim gestures, quest switcher, actionable view, side panel, block-reasons hover |
| 6 | Text sync | RON view, edit→validate→apply via event pipeline, error surfacing |
| 7 | Scale & polish | Viewport culling, incremental relayout, minimap, focus mode at 1k nodes |

## 9. Non-goals (v1)

- Auto-evaluated conditions (schema reserved only).
- Sync, collaboration, web/mobile.
- ANY / n-of-N dependency semantics.
- Nested graphs / containment hierarchies.
