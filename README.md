# Neutron

A dependency-graph task manager built in Rust on the [linebender](https://linebender.org)
stack (Xilem / Masonry / Vello). All tasks live in one global, flat, fully
cross-linkable graph; quests are *lenses* (claims) over that graph, not
containers. See [`PLAN.md`](./PLAN.md) for the full design.

## Workspace

| Crate | Purpose | Status |
|-------|---------|--------|
| `base` | Pure domain model + graph algorithms: nodes/edges/quests, readiness, Tarjan cycle detection, quest scope + actionable queries, the `Event` log reducer with inverse generation for undo. No I/O, no UI. | ✅ complete, property-tested |
| `db` | SQLite event log (append-only source of truth) + materialized projections + undo/redo via inverse events. One database for the whole global graph. | ✅ complete, round-trip tested |
| `layout` | Pure Sugiyama layered-DAG layout: feedback-arc cycle cut → longest-path ranking (top-down) → barycenter within-level ordering seeded by `order_hint` → coordinate assignment. | ✅ complete, tested |
| `neutron` | The Xilem app: a custom Masonry+Vello canvas widget (pan/zoom/hit-test/state styling), a side panel, and the command layer wiring gestures to events. | 🚧 canvas spike + editing |

## Milestone status (PLAN §8)

- **M1 Core model** — ✅ done. Edges first-class; quest claims + scope-closure &
  actionable queries; readiness + Tarjan SCC + cycle flagging, with property
  tests (Tarjan validated against an independent mutual-reachability oracle).
- **M2 Persistence** — ✅ done. SQLite event log, projections incl.
  `quest_claims`, undo/redo via inverse events (log stays monotonic),
  on-disk round-trip tests.
- **M3 Canvas spike** — ✅ done. Custom Masonry widget renders the graph with
  Vello, pan (drag) + zoom (wheel, about cursor), click hit-testing/selection,
  and per-state node styling; laid out by the real `layout` crate.
- **M4 Auto layout** — ◑ algorithm done and integrated (cycle cut + order-hint
  seeding). The *within-level reorder gesture* is not yet wired.
- **M5 Canvas editing** — ◑ add task/condition, rename, complete/satisfy,
  delete, add-requirement edges, claim/unclaim, quest switcher, actionable
  view, side panel are all wired. Canvas drag-to-create-edge and
  block-reason hover are not yet done (edges are added from the side panel).
- **M6 Text sync (RON)** — ☐ not started.
- **M7 Scale & polish** (culling, incremental relayout, minimap, focus) — ☐ not
  started.

## Build & run

The repo uses a Nix flake for the toolchain and system libraries (Vulkan,
fontconfig, wayland/xkb, …):

```sh
nix develop        # or: direnv allow
cargo test         # 34 tests across the workspace
cargo run -p neutron
```

The app opens or creates `neutron.db` in the working directory (override with
`NEUTRON_DB=/path/to.db`) and seeds a small demo graph on first run.

### Using it

- **Canvas**: drag to pan, scroll to zoom, click a node to select it. Task
  nodes are rounded rectangles, conditions are chamfered rectangles;
  border/fill encode Ready / Blocked / Completed / Cyclic / Pending.
  Dependency edges are solid, subtask edges dashed, and cycle-reversed edges
  are drawn in warning red.
- **Side panel**: create tasks/conditions, rename, toggle done, delete, add
  requirement edges to other nodes, and undo/redo. Switch quests to scope the
  view (pulled-in but unclaimed nodes render dimmed), claim/unclaim the
  selection, and read the **Actionable** list — "what can I do right now?".

## Notes

- The store is wrapped in a `Mutex` inside `AppState` because Xilem's
  `WidgetView` bound is `Send + Sync` and rusqlite's `Connection` is `!Sync`;
  the app is single-threaded so the lock is uncontended.
- Per the task, this build ignores the PLAN's references to a pre-existing
  `base` skeleton — every crate here is written from scratch.
