//! Persistence tests for the [`db::Store`] (PLAN Milestone 2): commit +
//! read-back, on-disk round-trip, projection consistency, and undo/redo
//! over a monotonic log.

use base::{EdgeId, EdgeKind, Event, NodeId, NodeKind, QuestId};
use db::Store;
use rusqlite::Connection;

fn nid(n: u128) -> NodeId { NodeId::from_u128(n) }

fn eid(n: u128) -> EdgeId { EdgeId::from_u128(n) }

fn qid(n: u128) -> QuestId { QuestId::from_u128(n) }

/// A representative batch touching every projection table.
fn sample_batch() -> Vec<Event> {
  vec![
    Event::NodeAdded {
      node:       nid(1),
      kind:       NodeKind::task(),
      name:       "root".into(),
      order_hint: 0.0,
    },
    Event::NodeAdded {
      node:       nid(2),
      kind:       NodeKind::task(),
      name:       "dep".into(),
      order_hint: 1.0,
    },
    Event::EdgeAdded {
      edge: eid(10),
      kind: EdgeKind::Dependency,
      from: nid(1),
      to:   nid(2),
    },
    Event::QuestCreated {
      quest: qid(100),
      name:  "epic".into(),
    },
    Event::QuestClaimed {
      quest: qid(100),
      node:  nid(1),
    },
    Event::TaskCompleted {
      node:      nid(2),
      completed: true,
    },
  ]
}

#[test]
fn commit_updates_the_in_memory_graph() {
  let mut store = Store::open_in_memory().unwrap();
  store.commit(sample_batch()).unwrap();

  let g = store.graph();
  assert_eq!(g.nodes().count(), 2);
  assert_eq!(g.edges().count(), 1);
  assert_eq!(g.quests().count(), 1);
  assert!(g.node(nid(2)).unwrap().kind.is_satisfied());
  assert!(g.quest(qid(100)).unwrap().claims.contains(&nid(1)));
}

#[test]
fn persists_and_reloads_from_disk() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("neutron.db");

  let before = {
    let mut store = Store::open(&path).unwrap();
    store.commit(sample_batch()).unwrap();
    store.graph().clone()
  }; // store dropped, connection closed

  let reopened = Store::open(&path).unwrap();
  assert_eq!(
    reopened.graph(),
    &before,
    "reloaded graph equals the pre-drop graph"
  );
}

#[test]
fn projection_tables_mirror_the_graph() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("neutron.db");
  let mut store = Store::open(&path).unwrap();
  store.commit(sample_batch()).unwrap();
  drop(store);

  // Inspect the raw projection tables with an independent connection.
  let conn = Connection::open(&path).unwrap();
  let count =
    |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get(0)).unwrap() };
  assert_eq!(count("SELECT COUNT(*) FROM nodes"), 2);
  assert_eq!(count("SELECT COUNT(*) FROM edges"), 1);
  assert_eq!(count("SELECT COUNT(*) FROM quests"), 1);
  assert_eq!(count("SELECT COUNT(*) FROM quest_claims"), 1);
}

#[test]
fn undo_and_redo_walk_the_graph_while_the_log_only_grows() {
  let mut store = Store::open_in_memory().unwrap();

  // Group 1: add a node.
  store
    .commit(vec![Event::NodeAdded {
      node:       nid(1),
      kind:       NodeKind::task(),
      name:       "a".into(),
      order_hint: 0.0,
    }])
    .unwrap();
  // Group 2: complete it.
  store
    .commit(vec![Event::TaskCompleted {
      node:      nid(1),
      completed: true,
    }])
    .unwrap();

  let log_after_commits = store.event_count().unwrap();
  assert!(store.can_undo());
  assert!(!store.can_redo());
  assert!(store.graph().node(nid(1)).unwrap().kind.is_satisfied());

  // Undo group 2: the task is incomplete again.
  store.undo().unwrap();
  assert!(!store.graph().node(nid(1)).unwrap().kind.is_satisfied());
  assert!(store.can_redo());

  // Redo group 2: complete once more.
  store.redo().unwrap();
  assert!(store.graph().node(nid(1)).unwrap().kind.is_satisfied());

  // History is monotonic: undo + redo each appended, never deleted.
  let log_after_undo_redo = store.event_count().unwrap();
  assert!(
    log_after_undo_redo > log_after_commits,
    "log grew from {log_after_commits} to {log_after_undo_redo}"
  );
}

#[test]
fn undo_of_node_removal_restores_edges_and_claims() {
  let mut store = Store::open_in_memory().unwrap();
  store.commit(sample_batch()).unwrap();
  let before = store.graph().clone();

  store
    .commit(vec![Event::NodeRemoved { node: nid(1) }])
    .unwrap();
  assert!(store.graph().node(nid(1)).is_none());
  assert_eq!(store.graph().edges().count(), 0);

  store.undo().unwrap();
  assert_eq!(store.graph(), &before, "removal fully restored by undo");
}

#[test]
fn settings_round_trip_and_survive_reopen() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("settings.db");

  {
    let store = Store::open(&path).unwrap();
    assert_eq!(store.setting("palette").unwrap(), None, "unset key is None");
    store.set_setting("palette", "umber").unwrap();
    // Writing again replaces rather than failing on the primary key.
    store.set_setting("palette", "frost").unwrap();
    assert_eq!(store.setting("palette").unwrap().as_deref(), Some("frost"));
  }

  let reopened = Store::open(&path).unwrap();
  assert_eq!(
    reopened.setting("palette").unwrap().as_deref(),
    Some("frost"),
    "preference outlived the connection"
  );
  // The migration's own meta row is untouched by preference writes.
  assert_eq!(
    reopened.setting("schema_version").unwrap().as_deref(),
    Some("1")
  );
}

#[test]
fn settings_are_not_events() {
  let store = Store::open_in_memory().unwrap();
  let before = store.event_count().unwrap();
  store.set_setting("palette", "graphite").unwrap();
  assert_eq!(
    store.event_count().unwrap(),
    before,
    "a preference must not touch the event log"
  );
  assert!(!store.can_undo(), "and must not be undoable");
}

#[test]
fn amended_commits_undo_as_one_group() {
  let mut store = Store::open_in_memory().unwrap();
  store.commit(sample_batch()).unwrap();
  let rename = |name: &str| Event::NodeRenamed {
    node: nid(1),
    name: name.into(),
  };
  store.commit(vec![rename("r")]).unwrap();
  store.commit_amend(vec![rename("re")]).unwrap();
  store.commit_amend(vec![rename("renamed")]).unwrap();
  assert_eq!(store.graph().node(nid(1)).unwrap().name, "renamed");

  // One undo backs out every amendment and the group they joined…
  store.undo().unwrap();
  assert_eq!(store.graph().node(nid(1)).unwrap().name, "root");
  // …and one redo restores the end state.
  store.redo().unwrap();
  assert_eq!(store.graph().node(nid(1)).unwrap().name, "renamed");
  store.undo().unwrap();
  // The group before the rename is untouched.
  store.undo().unwrap();
  assert!(store.graph().node(nid(1)).is_none());
}

#[test]
fn amending_with_nothing_to_amend_is_a_plain_commit() {
  let mut store = Store::open_in_memory().unwrap();
  store.commit_amend(sample_batch()).unwrap();
  assert!(store.can_undo());
  store.undo().unwrap();
  assert_eq!(store.graph().node_count(), 0);
}

#[test]
fn a_newer_schema_is_refused_and_left_untouched() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("neutron.db");
  {
    let mut store = Store::open(&path).unwrap();
    store.commit(sample_batch()).unwrap();
  }
  let conn = Connection::open(&path).unwrap();
  conn
    .execute(
      "UPDATE meta SET value = '99' WHERE key = 'schema_version'",
      [],
    )
    .unwrap();
  let events_before: i64 = conn
    .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
    .unwrap();
  drop(conn);

  match Store::open(&path) {
    Err(db::DbError::NewerSchema { found, .. }) => assert_eq!(found, 99),
    Err(e) => panic!("wrong error: {e}"),
    Ok(_) => panic!("a newer schema was opened"),
  }
  // Nothing was written: the version and the log are as the newer build
  // left them.
  let conn = Connection::open(&path).unwrap();
  let version: String = conn
    .query_row(
      "SELECT value FROM meta WHERE key = 'schema_version'",
      [],
      |r| r.get(0),
    )
    .unwrap();
  assert_eq!(version, "99");
  let events_after: i64 = conn
    .query_row("SELECT COUNT(*) FROM events", [], |r| r.get(0))
    .unwrap();
  assert_eq!(events_after, events_before);
}

#[test]
fn an_unreadable_event_is_reported_by_position() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("neutron.db");
  {
    let mut store = Store::open(&path).unwrap();
    store.commit(sample_batch()).unwrap();
  }
  let conn = Connection::open(&path).unwrap();
  conn
    .execute(
      "INSERT INTO events (id, payload) VALUES ('x', '{\"FromTheFuture\":{}}')",
      [],
    )
    .unwrap();
  let bad_seq: i64 = conn
    .query_row("SELECT MAX(seq) FROM events", [], |r| r.get(0))
    .unwrap();
  drop(conn);

  match Store::open(&path) {
    Err(db::DbError::BadEvent { seq, .. }) => assert_eq!(seq, bad_seq),
    Err(e) => panic!("wrong error: {e}"),
    Ok(_) => panic!("an unreadable event was skipped"),
  }
}

/// Overwrite the payload of the event at `seq` with something unreadable.
fn corrupt_event(path: &std::path::Path, seq: i64) {
  let conn = Connection::open(path).unwrap();
  conn
    .execute("UPDATE events SET payload = 'garbage' WHERE seq = ?1", [
      seq,
    ])
    .unwrap();
}

#[test]
fn opening_loads_the_snapshot_instead_of_replaying_it() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("neutron.db");
  let before = {
    let mut store = Store::open(&path).unwrap();
    store.commit(sample_batch()).unwrap();
    store.graph().clone()
  }; // closing writes the snapshot

  // An event the snapshot already covers is never read again, so breaking
  // it does not matter.
  corrupt_event(&path, 1);
  let reopened = Store::open(&path).unwrap();
  assert_eq!(reopened.graph(), &before);
}

#[test]
fn events_after_the_snapshot_are_replayed() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("neutron.db");
  {
    let mut store = Store::open(&path).unwrap();
    store.commit(sample_batch()).unwrap();
  }
  let after = {
    let mut store = Store::open(&path).unwrap();
    store
      .commit(vec![Event::NodeRenamed {
        node: nid(1),
        name: "renamed".into(),
      }])
      .unwrap();
    let graph = store.graph().clone();
    // A crash: the event is in the log, but the snapshot never caught up.
    std::mem::forget(store);
    graph
  };
  let reopened = Store::open(&path).unwrap();
  assert_eq!(reopened.graph(), &after);
  assert_eq!(reopened.graph().node(nid(1)).unwrap().name, "renamed");
}

#[test]
fn an_unreadable_snapshot_falls_back_to_a_full_replay() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("neutron.db");
  let before = {
    let mut store = Store::open(&path).unwrap();
    store.commit(sample_batch()).unwrap();
    store.graph().clone()
  };
  let conn = Connection::open(&path).unwrap();
  conn
    .execute("UPDATE nodes SET kind = 'garbage'", [])
    .unwrap();
  drop(conn);

  let reopened = Store::open(&path).unwrap();
  assert_eq!(reopened.graph(), &before);
}

#[test]
fn commits_do_not_rewrite_the_snapshot_every_time() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("neutron.db");
  let mut store = Store::open(&path).unwrap();
  store.commit(sample_batch()).unwrap();
  // Still open: the snapshot tables have not been touched by the commit.
  let conn = Connection::open(&path).unwrap();
  let nodes: i64 = conn
    .query_row("SELECT COUNT(*) FROM nodes", [], |r| r.get(0))
    .unwrap();
  assert_eq!(nodes, 0);
  drop(conn);

  // Enough commits and it catches up mid-session.
  for i in 0..db::SNAPSHOT_EVERY {
    store
      .commit(vec![Event::NodeRenamed {
        node: nid(1),
        name: format!("n{i}"),
      }])
      .unwrap();
  }
  let conn = Connection::open(&path).unwrap();
  let name: String = conn
    .query_row(
      "SELECT name FROM nodes WHERE id = ?1",
      [nid(1).to_string()],
      |r| r.get(0),
    )
    .unwrap();
  assert!(name.starts_with('n'), "snapshot caught up, got {name}");
}

#[test]
fn revision_moves_with_every_graph_change_and_only_then() {
  let mut store = Store::open_in_memory().unwrap();
  let mut seen = vec![store.revision()];
  let mut changed = |store: &Store| {
    assert!(!seen.contains(&store.revision()), "revision reused");
    seen.push(store.revision());
  };
  store.commit(sample_batch()).unwrap();
  changed(&store);
  let rename = Event::NodeRenamed {
    node: nid(1),
    name: "x".into(),
  };
  store.commit_amend(vec![rename]).unwrap();
  changed(&store);
  store.undo().unwrap();
  changed(&store);
  store.redo().unwrap();
  changed(&store);

  // Things that leave the graph alone leave the revision alone.
  let r = store.revision();
  store.set_setting("palette", "umber").unwrap();
  store.commit(vec![]).unwrap();
  assert_eq!(store.revision(), r);
}

#[test]
fn a_typed_rename_is_logged_once() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("neutron.db");
  let mut store = Store::open(&path).unwrap();
  store.commit(sample_batch()).unwrap();
  let before = store.event_count().unwrap();

  let rename = |name: &str| Event::NodeRenamed {
    node: nid(1),
    name: name.into(),
  };
  store.commit(vec![rename("S")]).unwrap();
  for typed in ["Sh", "Shi", "Ship"] {
    store.commit_amend(vec![rename(typed)]).unwrap();
  }
  assert_eq!(
    store.event_count().unwrap(),
    before + 1,
    "one row, rewritten"
  );
  assert_eq!(store.graph().node(nid(1)).unwrap().name, "Ship");

  // One undo still restores the name from before the edit.
  store.undo().unwrap();
  assert_eq!(store.graph().node(nid(1)).unwrap().name, "root");
  store.redo().unwrap();
  drop(store);

  // And the log replays to the same end state.
  let reopened = Store::open(&path).unwrap();
  assert_eq!(reopened.graph().node(nid(1)).unwrap().name, "Ship");
}

#[test]
fn history_is_never_rewritten() {
  let mut store = Store::open_in_memory().unwrap();
  store.commit(sample_batch()).unwrap();
  let rename = |name: &str| Event::NodeRenamed {
    node: nid(1),
    name: name.into(),
  };
  store.commit(vec![rename("a")]).unwrap();
  store.undo().unwrap();
  store.redo().unwrap();
  // The group is history now (its events were undone and redone), so an
  // amend appends rather than overwriting the redo's row.
  let before = store.event_count().unwrap();
  store.commit_amend(vec![rename("b")]).unwrap();
  assert_eq!(store.event_count().unwrap(), before + 1);

  // An amend that does not supersede the tail appends too.
  let before = store.event_count().unwrap();
  store
    .commit_amend(vec![Event::NodeRenamed {
      node: nid(2),
      name: "other".into(),
    }])
    .unwrap();
  assert_eq!(store.event_count().unwrap(), before + 1);
}

#[test]
fn rewriting_a_snapshotted_row_refreshes_the_snapshot() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("neutron.db");
  let rename = |name: &str| Event::NodeRenamed {
    node: nid(1),
    name: name.into(),
  };
  let mut store = Store::open(&path).unwrap();
  store.commit(sample_batch()).unwrap();
  // Pad the log so the next commit lands exactly on a snapshot.
  let pad = db::SNAPSHOT_EVERY - 1 - store.event_count().unwrap() as i64;
  for _ in 0..pad {
    store
      .commit(vec![Event::OrderHintChanged {
        node:       nid(2),
        order_hint: 1.0,
      }])
      .unwrap();
  }
  store.commit(vec![rename("first")]).unwrap(); // snapshot taken here
  store.commit_amend(vec![rename("final")]).unwrap(); // rewrites that row
  std::mem::forget(store); // no snapshot on close

  let reopened = Store::open(&path).unwrap();
  assert_eq!(reopened.graph().node(nid(1)).unwrap().name, "final");
}
