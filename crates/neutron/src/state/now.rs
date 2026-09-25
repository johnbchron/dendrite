//! The Now tray: what can be done right now (PLAN §2 actionable query).

use std::collections::HashSet;

use base::{Derived, Graph, NodeId, QuestId};

use super::AppState;

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
  ) -> (Vec<NowGroup>, usize) {
    let named = |ids: Vec<NodeId>| -> Vec<(NodeId, String)> {
      let mut v: Vec<(NodeId, String)> = ids
        .into_iter()
        .filter_map(|id| graph.node(id).map(|n| (id, n.name.clone())))
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

impl AppState {
  /// What can be done right now, for the Now tray, and how many distinct
  /// nodes that is; see [`NowGroup::collect`].
  pub fn now(&self) -> (Vec<NowGroup>, usize) {
    let store = self.lock();
    let cached = self.derivations(&store);
    NowGroup::collect(store.graph(), &cached.derived, self.active_quest)
  }

  /// Whether the Now tray is open.
  pub fn now_open(&self) -> bool { self.now_open }

  /// Open or close the Now tray.
  pub fn toggle_now(&mut self) { self.now_open = !self.now_open; }
}
