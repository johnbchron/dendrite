# Formula conditions

Status: **proposal**
Date: 2026-09-28
Author: John Lewis

Some conditions should be satisfied by the world, not by a click: *it is after 1 October*, *I'm at the hardware store*, *I have $50 to spend*, *I have an hour free*. This proposal fills the `ConditionSource` variant PLAN §2 reserved: conditions **computed from a formula over facts**, whose identity is **the formula itself**, so two tasks that need "at Home" share one node.

## At a glance

```text
One node per value, its truth computed from facts

┌──────────────────────┐            ┌──────────────────────┐            ┌──────────────────────┐
│ Referents            │ points at  │ Formula node         │  require   │ Tasks that need it   │
│ Places, resources,   │ ─────────▶ │ id = hash(atom)      │ ◀───────── │ Many edges, one node │
│ schedules, contexts  │            │ At(Home), Has($50)   │            │ Drawn as copies/row  │
│ Named, in event log  │            │ One node per value   │            │ Ordinary tasks       │
└──────────────────────┘            └──────────────────────┘            └──────────────────────┘
                                               │ evaluated
                                               ▼
┌──────────────────────┐            ┌──────────────────────┐            ┌──────────────────────┐
│ Facts                │   input    │ Derived::compute     │   drives   │ Readiness            │
│ Clock and time zone  │ ─────────▶ │ Graph + facts        │ ─────────▶ │ Ready or Blocked     │
│ Place, contexts,     │            │ Satisfaction and why │            │ Now tray and Soon    │
│   free time          │            │ Horizon: next flip   │            │ Inspector reasons    │
│ Preferences, not log │            │                      │            │                      │
└──────────────────────┘            └──────────────────────┘            └──────────────────────┘
           ▲                                   │
           └───────────────────────────────────┘
             a timer at the horizon refreshes the facts
```

Atoms point at named referents and are shared by every task that needs them; derivation reads the graph plus facts, and one timer set for the next flip keeps it current.

## Goals and non-goals

**Goals**

- A condition's satisfaction can come from a formula over *facts*: the clock, where I am, what I have, how much time I have, and which contexts are active.
- **Formula conditions deduplicate by value.** A formula has one node. Requiring a formula that already exists links to the existing node.
- Cover location, resources (money, time, anything countable), timing (start dates, end dates, one-off and recurring windows, time available right now), and plain contexts ("online", "at the computer", "with Sam").
- `base` stays pure. It never reads a clock, a GPS or a calendar. Facts are passed in.
- Readiness updates when facts change, including when time passes, without polling every frame.

**Non-goals**

- A general expression language (`money >= 50 && at(home) || ...`). The Model section explains why atoms stay small and conjunction stays in the graph.
- Consuming resources. Completing a task does not debit a balance.
- ANY / n-of-N semantics. PLAN §9 keeps them out, and nothing here needs them.
- Automatic fact sources such as OS geolocation or calendar free/busy. The design leaves room for them; v1 facts are declared by the user.
- Deadlines as urgency. A due date that *orders* work is a different feature from a window that *gates* work.

## Vocabulary

| Term | Meaning |
| --- | --- |
| **Atom** | One predicate over facts, for example `After(2026-10-01)` or `At(Home)`. The formula of a formula condition. |
| **Formula condition** | A condition node with `source: Formula(atom)`. The atom computes its satisfaction; a click never sets it. |
| **Referent** | A named, editable thing an atom points to: a Place, Resource, Schedule or Context. Graph data, in the event log. |
| **Facts** | A snapshot of the world at one moment: the time, current places, free time, active contexts. An input to derivation, never stored in the graph. |
| **Horizon** | The earliest future moment at which any atom's truth could change by time alone. |

## The model

An atom is one flat predicate; its node id is a hash of its canonical value; it points at named referents rather than embedding them.

### Atoms

```rust
/// One predicate over the facts. Deliberately flat: conjunction is
/// expressed by requiring several atoms, as every other requirement is.
#[serde(tag = "atom", rename_all = "snake_case")]
pub enum Atom {
  // timing
  After  { at: Moment },                 // now >= at: a start date
  Before { at: Moment },                 // now < at: the end of a gate
  Within { schedule: ScheduleId },       // one-off or recurring windows
  Free   { at_least: Minutes },          // declared free time left
  // location
  At     { place: PlaceId },
  // resources
  Has    { resource: ResourceId, at_least: Amount },
  // other
  In     { context: ContextId },         // "online", "with Sam"
}
```

**Why atoms and not expressions.** Deduplication only helps if equal meanings produce equal values. With expressions, `A && B`, `B && A` and two edges to `A` and `B` all mean the same thing, and only a normaliser could tell. The graph already has AND: a node needs *all* its requirements. So an atom does one thing, and composition happens in the graph, where it is shared and explained one line at a time ("Waiting on: after 1 Oct; at Home"). The loss is OR/NOT; place groups cover the common case, and the rest stays with PLAN's deferred ANY semantics.

### Canonical literals

Every literal has exactly one representation, so equal meanings hash equally.

| Type | Representation | Notes |
| --- | --- | --- |
| `Moment` | Civil date-time, minute precision, floating (no zone): `2026-10-01T09:00` | Read in the zone the facts carry, so "9am" means 9am wherever I am. A bare date means `T00:00`. |
| `Minutes` | `u32` | Free time is never meaningful below a minute. |
| `Amount` | `i64` in the resource's smallest unit (cents, minutes, items) | No floats in an atom: no `0.1 + 0.2` duplicates, no `NaN`. |
| ids | The referent's ULID | Identity is the referent, not its name or coordinates. |

`base` gains one pure dependency for civil time; [`jiff`](https://docs.rs/jiff) is recommended.

### Identity: content-addressed node ids

```rust
impl Atom {
  /// The id of the one node that holds this atom. Stable across builds,
  /// platforms and replicas.
  pub fn node_id(&self) -> NodeId {
    let hash  = blake3::hash(&self.canonical_bytes()); // not DefaultHasher
    let low80 = u128::from_le_bytes(hash[..16]) & ((1 << 80) - 1);
    NodeId::from_u128(low80) // top 48 bits (the ULID timestamp) are zero
  }
}
```

- **Deduplication falls out of identity.** "Require `At(Home)`" computes the id. If the node exists, only `EdgeAdded` is emitted; otherwise the group also holds `NodeAdded`. No lookup index, no uniqueness check, nothing to go stale.
- **It suits sync.** Two replicas that each add `After(2026-10-01)` produce the same node, and their logs merge without a duplicate.
- **The ids are recognisable.** A zero timestamp marks a value-addressed node; it sorts before every minted id and cannot collide with one. 80 bits of hash is plenty for one person's graph.
- `canonical_bytes` is versioned and frozen (a tag byte per variant, fixed-width fields), and a test pins known atoms to known ids. Changing it would silently fork every node.

### Referents

Atoms point at referents so a *definition* can change without changing identity. When I move house, I edit Home once; every `At(Home)` keeps its id, edges and layout position.

```rust
pub struct Place    { id: PlaceId, name: String, within: Option<PlaceId>, area: Option<Geofence> }
pub struct Resource { id: ResourceId, name: String, unit: Unit, balance: Amount }
pub struct Schedule { id: ScheduleId, name: String, spans: Vec<Span> }
pub struct Context  { id: ContextId, name: String }
```

`Graph` gains a map per kind, with events like quests have. A referent's name is how its atoms render, so formula nodes need no names of their own. The UI refuses to delete a referent that atoms still use, naming those nodes; the reducer stays total, and an atom with a missing referent evaluates to unsatisfied ("Unknown place").

### Where it sits in `NodeKind`

```rust
pub enum ConditionSource {
  Manual,
  Formula { atom: Atom },   // new
}
```

`Condition { satisfied, source }` keeps its shape; a formula condition's stored `satisfied` is ignored and kept `false`. The JSON change is additive, so old logs replay unchanged. An older build cannot parse a formula node, and `read_log` fails outright on it, so `SCHEMA_VERSION` goes to **3**: older builds then refuse the database cleanly with `NewerSchema`.

### Invariants

1. **Formula conditions are sinks.** They have no requirements; derivation ignores any edge from one, so it can never be `Cyclic`. The UI never offers "add requirement" on one.
2. **Formula conditions are never actionable.** They never enter `Derived::ready` or the Now tray. Today a pending condition with its requirements met is ready; that rule stays for manual conditions only.
3. **`ConditionSet` on a formula node is a no-op**, and the UI never emits one.
4. **Names are derived** from the atom and its referents. A user label (open question 3) would never be part of identity.
5. **Orphans are removed explicitly.** A gesture that removes the last edge into a formula node also emits `NodeRemoved` in the same group. Undo restores both through existing inverses; no hidden garbage collection at replay.

## The atoms in detail

Every atom evaluates to a truth, the moment it could next flip on time alone, and one line for the inspector.

```rust
pub struct Truth {
  pub holds: bool,
  /// The earliest moment this could flip with no other fact changing,
  /// or None if only a non-time fact can flip it.
  pub until: Option<Timestamp>,
  /// "Opens Thu 1 Oct (in 3 days)", "Needs $50.00, have $32.10".
  pub why:   Explanation,
}

impl Atom {
  pub fn eval(&self, graph: &Graph, facts: &Facts) -> Truth;
}
```

### Timing

| Atom | Holds when | `until` |
| --- | --- | --- |
| `After { at }` | `now >= at`, in the zone from the facts | `at` while it does not hold yet |
| `Before { at }` | `now < at` | `at` while it holds |
| `Within { schedule }` | `now` is in some span of the schedule | The next span boundary |
| `Free { at_least }` | `free_until - now >= at_least` | `free_until - at_least` while it holds |

Schedules are a list of spans, which covers windows without taking on RRULE:

```rust
pub enum Span {
  /// A one-off window: [start, end).
  Once   { start: Moment, end: Moment },
  /// Weekly: these weekdays, between these times of day. end < start
  /// wraps midnight ("Fri 22:00 – 02:00").
  Weekly { days: WeekdaySet, start: TimeOfDay, end: TimeOfDay },
}
```

- A one-off window can also be `After(a)` plus `Before(b)`: better when either end is shared with other work. A named schedule is better when the window has a meaning ("Conference week", "Business hours") or recurs.
- **`Before` is a gate, not a due date.** Once it passes, the task is Blocked for good. Right for "use the coupon before it expires", wrong for "file taxes by April 15". Deadlines get their own non-gating feature (see Later).
- **Free time is declared as "free until", not "free for".** "I have 45 minutes" goes stale the moment it is said; "free until 15:30" stays true and the clock does the arithmetic. With nothing declared, `Free` does not hold: "Free time unknown — set it in the Now tray".

### Location

`At { place }` holds when `place` is in `facts.places`, the set of places I am at. It is a set because places nest: at Home is also in Chicago.

- A place has an optional `within` parent; the facts close the set upward. That gives the common OR for free: "Errands" contains Hardware Store, Pharmacy and Grocer, so `At(Errands)` holds at any of them.
- A place's `area` (centre and radius) is optional and unused in v1, where the current place is picked by hand. It lets automatic location slot in later.
- `until` is `None`: moving is not a function of time.

### Resources

`Has { resource, at_least }` holds when `balance >= at_least`.

- A resource has a `Unit`: `Money { currency, minor_digits }`, `Minutes`, or `Count { noun }`. It drives display ("$50.00", "2 h", "3 batteries") and parsing.
- **The balance is graph data, in the log** (`ResourceBalanceSet`). I declare it ("$320 in the fun budget"), and it should survive restarts, sync, and undo like any edit.
- Different thresholds are different nodes: `Has(Money, 5000)` and `Has(Money, 2000)`. Every "$50" requirement still merges into one.
- **No allocation.** With $60, two tasks needing $50 are both Ready. A threshold is honest about what it checks; costs and budgets come later.
- "10 hours of contractor time left" is a resource with `Unit::Minutes`, distinct from `Free`, which is *my* time right now.

### Contexts

`In { context }` holds when `context` is in `facts.contexts`. This is the GTD-context catch-all: "@computer", "@phone", "online", "with Sam", "high energy", "raining". Contexts are defined in the log and toggled in the Now tray; `until` is `None`.

## Events

Referents get their own events following the patterns in `event.rs`; formula nodes need none.

| Event | Inverse | Supersedes |
| --- | --- | --- |
| `PlaceDefined { place, name, within }` | `PlaceRemoved` | — |
| `PlaceChanged { place, name, within }` | The previous values | The same place |
| `PlaceRemoved { place }` | `PlaceDefined` | — |
| `ResourceDefined { resource, name, unit, balance }` | `ResourceRemoved` | — |
| `ResourceBalanceSet { resource, balance }` | The previous balance | The same resource, so typing "320" logs once |
| `ResourceRenamed`, `ResourceRemoved` | As above | Rename: the same resource |
| `ScheduleDefined`, `ScheduleChanged`, `ScheduleRemoved` | As above | Changed: the same schedule |
| `ContextDefined`, `ContextRenamed`, `ContextRemoved` | As above | Rename: the same context |

A formula node is `NodeAdded { kind: Condition { source: Formula { atom } } }` with the content-addressed id. The one reducer change: `NodeAdded` on an existing formula id **does not replace** the node, so two replicas, or an undo and redo, adding the same atom are idempotent. Current place, active contexts and free-until are not events.

## Facts and derivation

Derivation takes facts as an input, and one timer set for the horizon keeps it current without polling.

### Facts

```rust
pub struct Facts {
  pub now:        Timestamp,          // an instant
  pub zone:       TimeZone,           // to read floating Moments in
  pub places:     HashSet<PlaceId>,   // closed upward over `within`
  pub contexts:   HashSet<ContextId>,
  pub free_until: Option<Timestamp>,
  // Resource balances are read from the graph.
}
```

| What | Where | Why |
| --- | --- | --- |
| Places, resources, schedules, contexts (definitions) | Event log | Edits: undoable, synced |
| Resource balances | Event log | Declared state I want kept and undoable |
| Now, zone | Clock | Observation |
| Current place, active contexts, free-until | Preferences (`Backend::set_setting`) | Observations of this device at this moment. They would flood undo and the audit trail, and must not sync: phone and laptop can be in different places. They persist across restarts. |

### Derivation and time

`Derived::compute(graph)` becomes `Derived::compute(graph, facts)`.

- Satisfaction moves into `Derived`: the stored bit for tasks and manual conditions, the atom for formula conditions. `Graph::is_satisfied` stays for callers that mean the stored bit. The one that means "satisfied for gating", `Reason::for_node` in `app/src/state/selection.rs`, switches to `Derived`.
- `Derived` records each formula node's `Truth`, so the inspector shows `why` without evaluating again.
- `Derived::horizon()` is the minimum `until` over all formula nodes.

In `app`, `Caches::derivations` is keyed on `(revision, facts_revision)` instead of `revision` alone:

1. `facts_revision` bumps when a declared fact changes, or when the clock passes the horizon.
2. The app arms one timer for the horizon (the `widgets` crate already has a keyed timer). When it fires, the app bumps `facts_revision` and recomputes, which yields the next horizon. A graph with no time atoms never wakes up.
3. A once-a-minute safety tick and a recompute on window focus catch clock jumps (sleep, DST, manual changes) that a single timer can miss.

`now` is fixed within one `Facts`, so a whole derivation sees one instant and tests pass a literal one. Layout does not depend on facts: satisfaction changes styling, not rank, so time passing moves nothing, and the lens and layout caches stay keyed on the graph revision.

### Fact sources

v1 has the system clock and what the user declares in the Now tray, behind a small trait in `app`:

```rust
pub trait FactSource {
  fn contribute(&self, facts: &mut Facts);
  fn next_change(&self) -> Option<Timestamp>;   // folded into the horizon
}
```

That leaves room, without committing to them, for OS geolocation or a phone companion, calendar free/busy feeding `free_until`, and a focus-mode integration feeding contexts.

## UI

One node in the model, a copy per place it is drawn, and the Now tray is where facts are declared.

**Drawing.** Formula conditions keep the chamfered condition shape and add a category glyph (Lucide `clock`, `calendar-range`, `map-pin`, `wallet`, `tag`, `hourglass`; `scripts/subset-icons.sh` grows to include them). The label renders from the atom: "After Thu 1 Oct", "At Home", "Has $50.00". Because formula nodes are sinks, `layout/src/copies.rs` already draws one per (tree, row) of their dependents. Deduplication in the model and duplication in the drawing complement each other: one node keeps the truth consistent, and the copies stop "At Home" pulling every errand into one tree.

**Adding.** The requirement search (link mode and the palette) parses a small phrase grammar as you type, and offers the atom beside existing nodes.

| Typed | Offered |
| --- | --- |
| `after oct 1`, `from friday`, `starting 2026-10-01` | `After` |
| `before oct 15`, `until sunday` | `Before` |
| `during business hours`, `weekends` | `Within`, matching schedules by name, with "New schedule…" |
| `at home`, `@hardware store` | `At`, with "New place…" |
| `$50`, `have $50`, `3 batteries` | `Has`, matching resources by unit and name |
| `1h free`, `30m` | `Free` |
| `online`, `with sam` | `In` |

When the atom exists already, the match says so ("At Home — used by 6"), and choosing it links to that node, which makes deduplication visible.

**Inspector.** On a formula node: its `why` ("Opens Thu 1 Oct, in 3 days", "You're at Office"), what it gates, and instead of Satisfy, a primary action that edits the fact: "Set place…", "Set balance…", "Edit schedule…". On a Blocked task, "Waiting on…" shows each formula requirement's `why` inline.

**Now tray.** A context bar on top: current place (picker), free until (presets plus a time field), and context chips. A **Soon** section lists tasks blocked *only* by time atoms, ordered by when they open ("Opens in 2 h: Call the bank"), straight from `Truth::until`.

**Keys.** One new binding: `C` opens the place picker, the most frequent fact change.

## Plan

Four steps, each landing green and useful on its own; PLAN §2 and §9 are updated when F1 lands.

| Step | Scope | Done when |
| --- | --- | --- |
| F1 Atoms in `base` | `Atom`, canonical literals, `node_id`, `eval`, `Facts`, `Derived::compute(graph, facts)` with `horizon`; referents as graph maps with events and inverses | Property tests: `node_id` stable (pinned vectors) and injective over generated atoms; equal atoms dedupe through `NodeAdded`; `Before`/`After` flip exactly at `until`; weekly spans across midnight and DST; horizon is the minimum; formula nodes never Ready or Cyclic |
| F2 Persistence | Schema v3, round-trip of every new event, idempotent re-add, orphan removal in one undo group | `db/tests/store.rs` covers undo/redo of requiring an existing atom (edge only) and a new one (node and edge) |
| F3 App | Fact store in preferences, `facts_revision`, horizon timer, `Reason` via `Derived`, the Soon section, the phrase parser | With an injected clock, passing the horizon changes the Now tray and nothing else recomputes |
| F4 UI | Glyphs, context bar, atom matches in the requirement search, referent editors as inspector cards | "at home" on two tasks draws two copies of one node; setting the place in the context bar readies both |

## Later, and explicitly not now

- **Due dates:** a non-gating `due: Option<Moment>` on tasks that sorts the Now tray and colours urgency. This is where deadlines belong, not `Before`.
- **Costs:** a task declares `costs: Vec<(ResourceId, Amount)>`; completing it emits `ResourceBalanceSet` in the same group, so undo refunds it. Allocation across Ready tasks stays open.
- **Graph-derived atoms:** `Elapsed { since: NodeId, at_least: Minutes }` for "cure 48 h after painting". Needs `completed_at` on tasks; the log has it (event ids are ULIDs), the `Graph` does not keep it yet.
- **Automatic facts:** geolocation and calendar free/busy, through `FactSource`.
- **OR / NOT:** `Not(Atom)` canonicalises easily; `AnyOf` brings back the normalisation problem. Both stay with PLAN's deferred ANY semantics.

## Open questions

1. **Floating or zoned moments.** Every `Moment` here is floating, read in the device's current zone. Is there a real case for "9am New York time"? If so, `Moment` gains an optional zone; leaving it out keeps the canonical form simple.
2. **Remove orphan formula nodes at all?** Removing them keeps unused atoms off the canvas. Keeping and hiding them makes them reusable from the palette. Removal is simpler, and re-typing is cheap since the parser recreates the same id.
3. **User labels on formula nodes.** "Tax season opens" reads better than "After Sat 1 Feb". A last-write-wins label outside identity is easy, but two replicas labelling one atom differently lose one silently. Label atoms, or only referents?
4. **Free-time granularity.** Tasks needing 15, 20, 25 and 30 minutes make four nodes. Right, or should the parser snap to a scale (15m, 30m, 1h, 2h, half a day)?
5. **Keep `Before`?** It exists for real gates like expiring coupons, but store hours fit `Within` better, and it is the atom most likely misused as a deadline. Drop it from v1 until a real gate needs it?
