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
| `layout` | Pure Sugiyama layered-DAG layout: feedback-arc cycle cut → longest-path ranking (top-down) → barycenter within-level ordering seeded by `order_hint`, with long edges given a reserved channel in every rank they skip → Brandes–Köpf coordinate assignment. | ✅ complete, tested |
| `neutron` | The Xilem app: a custom Masonry+Vello canvas widget (pan/zoom/hit-test/state styling, link-mode feedback), floating chrome over it (top bar, inspector card, Now tray, quest switcher, command palette), a root key map, and the command layer wiring gestures to events. | 🚧 editing done, text sync to come |

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
  delete (with undo), requirement edges by clicking nodes in link mode,
  claim/unclaim, quest switcher, actionable view, block reasons in the
  inspector, command palette and a full key map are all wired. Drag-to-create
  edges on the canvas is not done (link mode covers it).
- **M6 Text sync (RON)** — ☐ not started.
- **M7 Scale & polish** — ◑ viewport culling, per-revision caching of
  derived state and layout, and snapshot loading are done; incremental
  relayout, the minimap and focus mode are not started.

## Build & run

The repo uses a Nix flake for the toolchain and system libraries (Vulkan,
fontconfig, wayland/xkb, …):

```sh
nix develop        # or: direnv allow
cargo test         # 118 tests across the workspace
cargo run -p neutron
```

The app opens or creates `neutron/neutron.db` in the platform data directory
(e.g. `~/.local/share` on Linux; override with `NEUTRON_DB=/path/to.db`). A new
database starts empty.

### Using it

The canvas fills the window; everything else floats over it.

- **Canvas**: drag to pan, scroll to zoom, click a node to select it. Tasks
  are rounded rectangles, conditions chamfered ones; fill and border encode
  Ready / Blocked / Completed / Cyclic / Pending. Edges are dependencies, and
  cycle-reversed edges are drawn in warning red.
- **Top bar**: the quest lens (switch, search, create and rename quests),
  new task / condition (attached to the selection as its requirement, if
  there is one), a search box for the command palette, undo / redo (their
  tooltips name the step), zoom and fit, and settings (the colour palette).
- **Inspector**: appears while a node is selected. It says why the node is
  in its state ("Waiting on…", "In a cycle with…", each a link), leads with
  one primary action (complete, reopen, satisfy), and lists requirements and
  dependents; click one to go to it. The requirement search arms link mode:
  click nodes on the canvas (Shift+click for several) or pick a match.
- **Now tray** (bottom left): everything actionable right now, grouped by
  quest in the global view.

### Keys

Single letters work while no text field has focus. Ctrl is Cmd on macOS.

| Key | Action |
|-----|--------|
| Ctrl+K | Command palette: nodes, commands, quests |
| / | Palette, nodes only |
| N / Shift+N | New task / condition (a requirement of the selection), with its name selected to type over |
| R | Add a requirement (link mode) |
| Space | The selection's primary action |
| Enter or F2 | Rename the selection |
| Delete / Backspace | Delete the selection (undo from the toast) |
| Arrow keys | Move the selection: up to a dependent, down to a requirement, along its row |
| Escape | Leave a field, then link mode, then a popover, then the selection |
| Ctrl+Z / Ctrl+Shift+Z | Undo / redo |
| Ctrl+= / Ctrl+- / Ctrl+0 | Zoom in / out / 100% |
| F | Fit the graph |
| Q | Quest switcher |
| A | Now tray |

## Notes

- The store is wrapped in a `Mutex` inside `AppState` because Xilem's
  `WidgetView` bound is `Send + Sync` and rusqlite's `Connection` is `!Sync`;
  the app is single-threaded so the lock is uncontended.
- Per the task, this build ignores the PLAN's references to a pre-existing
  `base` skeleton — every crate here is written from scratch.
