//! The snapshot tables: `nodes`, `edges`, `quests` and `quest_claims`
//! mirror the graph as of the log position recorded under
//! [`SNAPSHOT_KEY`] in `meta`.

use base::{Edge, Graph, Node, Quest};
use rusqlite::Connection;

use crate::{DbError, meta::Meta};

/// `meta` key holding the log position the snapshot tables reflect.
const SNAPSHOT_KEY: &str = "snapshot_seq";

/// Reads and writes the snapshot over a borrowed connection (or
/// transaction).
pub(crate) struct Snapshot<'c>(pub(crate) &'c Connection);

impl Snapshot<'_> {
  /// The snapshot graph and the log position it reflects, or `None` when
  /// there is no usable snapshot: none recorded, one claiming a position
  /// past the end of the log, or rows that do not parse. The caller then
  /// replays from the start, so a bad snapshot costs time, never data.
  pub(crate) fn load(&self) -> Result<Option<(Graph, i64)>, DbError> {
    let Some(seq) = Meta(self.0)
      .get(SNAPSHOT_KEY)?
      .and_then(|v| v.parse::<i64>().ok())
    else {
      return Ok(None);
    };
    let max: i64 = self.0.query_row(
      "SELECT COALESCE(MAX(seq), 0) FROM events",
      [],
      |r| r.get(0),
    )?;
    if seq > max {
      return Ok(None);
    }
    Ok(self.read_graph().map(|g| (g, seq)))
  }

  /// Rebuild a graph from the projection tables, or `None` if any row is
  /// unreadable.
  fn read_graph(&self) -> Option<Graph> {
    let mut graph = Graph::new();

    let mut stmt = self
      .0
      .prepare("SELECT id, name, kind, order_hint FROM nodes")
      .ok()?;
    let rows = stmt
      .query_map([], |r| {
        Ok((
          r.get::<_, String>(0)?,
          r.get::<_, String>(1)?,
          r.get::<_, String>(2)?,
          r.get::<_, f64>(3)?,
        ))
      })
      .ok()?;
    for row in rows {
      let (id, name, kind, hint) = row.ok()?;
      let kind = serde_json::from_str(&kind).ok()?;
      graph.insert_node(Node::new(id.parse().ok()?, name, kind, hint));
    }

    let mut stmt = self
      .0
      .prepare("SELECT id, kind, from_node, to_node FROM edges")
      .ok()?;
    let rows = stmt
      .query_map([], |r| {
        Ok((
          r.get::<_, String>(0)?,
          r.get::<_, String>(1)?,
          r.get::<_, String>(2)?,
          r.get::<_, String>(3)?,
        ))
      })
      .ok()?;
    for row in rows {
      let (id, kind, from, to) = row.ok()?;
      let kind = serde_json::from_str(&kind).ok()?;
      graph.insert_edge(Edge::new(
        id.parse().ok()?,
        kind,
        from.parse().ok()?,
        to.parse().ok()?,
      ));
    }

    let mut stmt = self.0.prepare("SELECT id, name FROM quests").ok()?;
    let rows = stmt
      .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
      .ok()?;
    for row in rows {
      let (id, name) = row.ok()?;
      graph.insert_quest(Quest::new(id.parse().ok()?, name));
    }

    let mut stmt = self
      .0
      .prepare("SELECT quest_id, node_id FROM quest_claims")
      .ok()?;
    let rows = stmt
      .query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))
      .ok()?;
    for row in rows {
      let (quest, node) = row.ok()?;
      graph.claim(quest.parse().ok()?, node.parse().ok()?);
    }

    Some(graph)
  }

  /// Rewrite the snapshot tables from `graph` and record that they reflect
  /// the log up to `seq`.
  pub(crate) fn save(&self, graph: &Graph, seq: i64) -> Result<(), DbError> {
    self.write_graph(graph)?;
    Meta(self.0).set(SNAPSHOT_KEY, &seq.to_string())
  }

  /// Overwrite every projection table to exactly mirror `graph`.
  ///
  /// A full rewrite is trivially correct and more than fast enough at v1
  /// scale; incremental projection maintenance can replace it later.
  fn write_graph(&self, graph: &Graph) -> Result<(), DbError> {
    let conn = self.0;
    conn.execute_batch(
      "DELETE FROM nodes;
       DELETE FROM edges;
       DELETE FROM quests;
       DELETE FROM quest_claims;",
    )?;

    {
      let mut stmt = conn.prepare(
        "INSERT INTO nodes (id, name, kind, order_hint) VALUES (?1, ?2, ?3, \
         ?4)",
      )?;
      for node in graph.nodes() {
        let kind = serde_json::to_string(&node.kind)?;
        stmt.execute((
          node.id.to_string(),
          &node.name,
          kind,
          node.order_hint,
        ))?;
      }
    }
    {
      let mut stmt = conn.prepare(
        "INSERT INTO edges (id, kind, from_node, to_node) VALUES (?1, ?2, ?3, \
         ?4)",
      )?;
      for edge in graph.edges() {
        let kind = serde_json::to_string(&edge.kind)?;
        stmt.execute((
          edge.id.to_string(),
          kind,
          edge.from.to_string(),
          edge.to.to_string(),
        ))?;
      }
    }
    {
      let mut quest_stmt =
        conn.prepare("INSERT INTO quests (id, name) VALUES (?1, ?2)")?;
      let mut claim_stmt = conn.prepare(
        "INSERT INTO quest_claims (quest_id, node_id) VALUES (?1, ?2)",
      )?;
      for quest in graph.quests() {
        quest_stmt.execute((quest.id.to_string(), &quest.name))?;
        for node in &quest.claims {
          claim_stmt.execute((quest.id.to_string(), node.to_string()))?;
        }
      }
    }
    Ok(())
  }
}
