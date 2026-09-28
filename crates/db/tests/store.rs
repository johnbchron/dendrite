//! Tests for the SQLite backend and the session it drives (PLAN Milestone
//! 2): commit + read-back, on-disk round-trip, and undo/redo over a
//! monotonic log.

use base::{EdgeId, EdgeKind, Event, NodeId, NodeKind, QuestId};
use rusqlite::Connection;

fn nid(n: u128) -> NodeId { NodeId::from_u128(n) }

fn eid(n: u128) -> EdgeId { EdgeId::from_u128(n) }

fn qid(n: u128) -> QuestId { QuestId::from_u128(n) }

/// The `DbError` behind a boxed session error. Opening reports SQLite's own
/// errors; the session only passes them along.
fn db_error(e: session::Error) -> db::DbError {
  *e.downcast::<db::DbError>()
    .unwrap_or_else(|e| panic!("not a db error: {e}"))
}

/// A representative batch touching nodes, edges and quests.
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
  let mut store = db::open_in_memory().unwrap();
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
  let path = dir.path().join("dendrite.db");

  let before = {
    let mut store = db::open(&path).unwrap();
    store.commit(sample_batch()).unwrap();
    store.graph().clone()
  }; // store dropped, connection closed

  let reopened = db::open(&path).unwrap();
  assert_eq!(
    reopened.graph(),
    &before,
    "reloaded graph equals the pre-drop graph"
  );
}

#[test]
fn undo_and_redo_walk_the_graph_while_the_log_only_grows() {
  let mut store = db::open_in_memory().unwrap();

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
fn settings_round_trip_and_survive_reopen() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("settings.db");

  {
    let store = db::open(&path).unwrap();
    assert_eq!(store.setting("palette").unwrap(), None, "unset key is None");
    store.set_setting("palette", "umber").unwrap();
    // Writing again replaces rather than failing on the primary key.
    store.set_setting("palette", "frost").unwrap();
    assert_eq!(store.setting("palette").unwrap().as_deref(), Some("frost"));
  }

  let reopened = db::open(&path).unwrap();
  assert_eq!(
    reopened.setting("palette").unwrap().as_deref(),
    Some("frost"),
    "preference outlived the connection"
  );
  // The migration's own meta row is untouched by preference writes.
  assert_eq!(
    reopened.setting("schema_version").unwrap().as_deref(),
    Some("2")
  );
}

#[test]
fn settings_are_not_events() {
  let store = db::open_in_memory().unwrap();
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
  let mut store = db::open_in_memory().unwrap();
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
  let mut store = db::open_in_memory().unwrap();
  store.commit_amend(sample_batch()).unwrap();
  assert!(store.can_undo());
  store.undo().unwrap();
  assert_eq!(store.graph().node_count(), 0);
}

#[test]
fn a_newer_schema_is_refused_and_left_untouched() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("dendrite.db");
  {
    let mut store = db::open(&path).unwrap();
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

  match db::open(&path) {
    Err(e) => match db_error(e) {
      db::DbError::NewerSchema { found, .. } => assert_eq!(found, 99),
      e => panic!("wrong error: {e}"),
    },
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
  let path = dir.path().join("dendrite.db");
  {
    let mut store = db::open(&path).unwrap();
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

  match db::open(&path) {
    Err(e) => match db_error(e) {
      db::DbError::BadEvent { seq, .. } => assert_eq!(seq, bad_seq),
      e => panic!("wrong error: {e}"),
    },
    Ok(_) => panic!("an unreadable event was skipped"),
  }
}

#[test]
fn a_v1_database_sheds_its_projection_tables() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("dendrite.db");
  let before = {
    let mut store = db::open(&path).unwrap();
    store.commit(sample_batch()).unwrap();
    store.graph().clone()
  };
  // Put the database back into its v1 shape: the projection tables, the
  // position they claimed to hold, and the old version.
  let conn = Connection::open(&path).unwrap();
  conn
    .execute_batch(
      "CREATE TABLE nodes (id TEXT PRIMARY KEY);
       CREATE TABLE edges (id TEXT PRIMARY KEY);
       CREATE TABLE quests (id TEXT PRIMARY KEY);
       CREATE TABLE quest_claims (quest_id TEXT, node_id TEXT);
       INSERT INTO meta (key, value) VALUES ('snapshot_seq', '1');
       UPDATE meta SET value = '1' WHERE key = 'schema_version';",
    )
    .unwrap();
  drop(conn);

  let reopened = db::open(&path).unwrap();
  assert_eq!(
    reopened.graph(),
    &before,
    "the log still rebuilds the graph"
  );
  assert_eq!(
    reopened.setting("schema_version").unwrap().as_deref(),
    Some("2")
  );
  assert_eq!(reopened.setting("snapshot_seq").unwrap(), None);
  drop(reopened);

  let conn = Connection::open(&path).unwrap();
  let tables: i64 = conn
    .query_row(
      "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name IN \
       ('nodes', 'edges', 'quests', 'quest_claims')",
      [],
      |r| r.get(0),
    )
    .unwrap();
  assert_eq!(tables, 0, "the projection tables are gone");
}

#[test]
fn every_committed_event_is_replayed_on_reopening() {
  let dir = tempfile::tempdir().unwrap();
  let path = dir.path().join("dendrite.db");
  let after = {
    let mut store = db::open(&path).unwrap();
    store.commit(sample_batch()).unwrap();
    store
      .commit(vec![Event::NodeRenamed {
        node: nid(1),
        name: "renamed".into(),
      }])
      .unwrap();
    store.graph().clone()
  };
  let reopened = db::open(&path).unwrap();
  assert_eq!(reopened.graph(), &after);
  assert_eq!(reopened.graph().node(nid(1)).unwrap().name, "renamed");
}

#[test]
fn revision_moves_with_every_graph_change_and_only_then() {
  let mut store = db::open_in_memory().unwrap();
  let mut seen = vec![store.revision()];
  let mut changed = |store: &db::Store| {
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
  let path = dir.path().join("dendrite.db");
  let mut store = db::open(&path).unwrap();
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
  let reopened = db::open(&path).unwrap();
  assert_eq!(reopened.graph().node(nid(1)).unwrap().name, "Ship");
}

#[test]
fn history_is_never_rewritten() {
  let mut store = db::open_in_memory().unwrap();
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
fn undo_and_redo_name_the_step_they_would_take() {
  let mut store = db::open_in_memory().unwrap();
  assert_eq!(store.undo_label(), None);
  store.commit(sample_batch()).unwrap();
  assert_eq!(store.undo_label(), Some("add task"));
  let rename = |name: &str| Event::NodeRenamed {
    node: nid(1),
    name: name.into(),
  };
  store.commit(vec![rename("a")]).unwrap();
  // Amending keeps the group's name.
  store.commit_amend(vec![rename("ab")]).unwrap();
  assert_eq!(store.undo_label(), Some("rename"));

  store.undo().unwrap();
  assert_eq!(store.undo_label(), Some("add task"));
  assert_eq!(store.redo_label(), Some("rename"));
  store.redo().unwrap();
  assert_eq!(store.undo_label(), Some("rename"));
  assert_eq!(store.redo_label(), None);
}
