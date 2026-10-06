//! The Now tray: what can be done right now (PLAN §2 actionable query), and
//! what the clock alone is holding back.

use std::collections::HashSet;

use base::{Derived, Graph, NodeId, NodeKind, NodeState, QuestId};
use jiff::{Timestamp, civil::Date};

use super::AppState;
use crate::formula::describe;

/// A group of actionable nodes in the Now tray.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NowGroup {
  /// The quest (or "Not in a quest") the group is for; `None` when there is
  /// only one group and nothing to tell it apart from.
  pub title: Option<String>,
  /// The nodes, by name.
  pub items: Vec<(NodeId, String)>,
}

impl NowGroup {
  /// The groups for the lens of `quest` (or the global view), and how many
  /// distinct nodes they hold.
  ///
  /// In a quest lens, one untitled group: the quest's actionable frontier.
  /// In the global view, one group per quest with any actionable work (a
  /// node in several quests' scopes appears under each), then the Ready
  /// nodes no quest reaches, under "Not in a quest".
  fn collect(
    graph: &Graph,
    derived: &Derived,
    quest: Option<QuestId>,
    today: Date,
  ) -> (Vec<NowGroup>, usize) {
    let named = |ids: Vec<NodeId>| -> Vec<(NodeId, String)> {
      let mut v: Vec<(NodeId, String)> = ids
        .into_iter()
        .filter_map(|id| {
          graph
            .node(id)
            .map(|n| (id, describe::node_name(graph, n, today)))
        })
        .collect();
      v.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
      v
    };

    if let Some(q) = quest {
      let items = named(base::actionable(graph, derived, q));
      let total = items.len();
      return (vec![NowGroup { title: None, items }], total);
    }

    let mut quests: Vec<_> = graph.quests().collect();
    quests.sort_by(|a, b| a.name.cmp(&b.name).then(a.id.cmp(&b.id)));
    let mut reached = HashSet::new();
    let mut groups = Vec::new();
    for quest in quests {
      let ids = base::actionable(graph, derived, quest.id);
      reached.extend(ids.iter().copied());
      if !ids.is_empty() {
        groups.push(NowGroup {
          title: Some(quest.name.clone()),
          items: named(ids),
        });
      }
    }
    let ready: Vec<NodeId> = derived.ready_nodes().iter().copied().collect();
    let total = ready.len();
    let loose: Vec<NodeId> =
      ready.into_iter().filter(|n| !reached.contains(n)).collect();
    if !loose.is_empty() {
      // Titled only when there are quest groups to tell it apart from.
      let title = (!groups.is_empty()).then(|| "Not in a quest".to_string());
      groups.push(NowGroup {
        title,
        items: named(loose),
      });
    }
    (groups, total)
  }
}

/// A task nothing but the clock is holding back, for the tray's Soon
/// section.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoonItem {
  /// The task.
  pub node: NodeId,
  /// Its name.
  pub name: String,
  /// The earliest it can open: when its last time requirement flips.
  pub opens: Timestamp,
  /// That, from now: "in 2 h".
  pub when: String,
}

impl AppState {
  /// Tasks blocked only by requirements that time alone will meet, soonest
  /// first, within the active lens. Nothing in the completed lens, where
  /// all the work is done.
  pub fn soon(&self) -> Vec<SoonItem> {
    if self.completed_lens {
      return Vec::new();
    }
    let store = self.lock();
    let graph = store.graph();
    let cached = self.derivations(&store);
    let derived = &cached.derived;
    let now = cached.facts.now;
    let today = cached.facts.zone.to_datetime(now).date();
    let scope = self.active_quest.map(|q| base::scope(graph, q));

    let mut items: Vec<SoonItem> = graph
      .nodes()
      .filter(|n| matches!(n.kind, NodeKind::Task { .. }))
      .filter(|n| derived.state(n.id) == Some(NodeState::Blocked))
      .filter(|n| scope.as_ref().is_none_or(|s| s.contains(n.id)))
      .filter_map(|n| {
        // Every unmet requirement must be one the clock will flip.
        let opens = graph
          .requirements_of(n.id)
          .filter(|e| !derived.is_satisfied(e.to))
          .map(|e| derived.truth(e.to).and_then(|t| t.until))
          .try_fold(None, |latest: Option<Timestamp>, until| {
            Some(latest.max(Some(until?)))
          })??;
        Some(SoonItem {
          node: n.id,
          name: describe::node_name(graph, n, today),
          opens,
          when: describe::relative(now, opens),
        })
      })
      .collect();
    items.sort_by(|a, b| {
      (a.opens, &a.name, a.node).cmp(&(b.opens, &b.name, b.node))
    });
    items
  }

  /// What can be done right now, for the Now tray, and how many distinct
  /// nodes that is; see `NowGroup::collect`. Nothing in the completed
  /// lens.
  pub fn now(&self) -> (Vec<NowGroup>, usize) {
    if self.completed_lens {
      return (Vec::new(), 0);
    }
    let store = self.lock();
    let cached = self.derivations(&store);
    NowGroup::collect(
      store.graph(),
      &cached.derived,
      self.active_quest,
      self.today(),
    )
  }

  /// Whether the Now tray is open.
  pub fn now_open(&self) -> bool {
    self.now_open
  }

  /// Open or close the Now tray.
  pub fn toggle_now(&mut self) {
    self.now_open = !self.now_open;
  }
}
