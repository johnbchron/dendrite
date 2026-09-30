//! A typed query over a list: the text, which result is highlighted, and
//! how well an item matches.
//!
//! The quest switcher and the command palette each type into a search
//! field; the arrow keys pass through it (it is single-line) to move the
//! highlight here, via the key map.
//!
//! Matching is delegated to [`nucleo_matcher`], the fzf-style matcher behind
//! helix: it ranks by word boundaries and gaps rather than plain substrings,
//! and handles non-ASCII correctly. Its [`Matcher`] owns ~135KB of scratch
//! memory, so one is kept per thread and reused across calls.

use std::{cell::RefCell, cmp::Ordering};

use nucleo_matcher::{
  Config, Matcher, Utf32Str,
  pattern::{CaseMatching, Normalization, Pattern},
};

/// A query's text and the highlighted result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
  /// What has been typed.
  pub text:      String,
  /// Index of the highlighted result.
  pub highlight: usize,
}

impl Query {
  /// Replace the text, as typed into the search field. A change puts the
  /// highlight back on the first (best) result.
  pub fn set_text(&mut self, text: String) {
    if text != self.text {
      self.text = text;
      self.highlight = 0;
    }
  }

  /// Move the highlight by `by` among `len` results, stopping at the ends.
  pub fn move_highlight(&mut self, by: isize, len: usize) {
    if len == 0 {
      self.highlight = 0;
      return;
    }
    let last = len as isize - 1;
    self.highlight = (self.highlight as isize + by).clamp(0, last) as usize;
  }

  /// The highlight, kept within `len` results.
  pub fn highlighted(&self, len: usize) -> usize {
    self.highlight.min(len.saturating_sub(1))
  }
}

thread_local! {
  /// One matcher (and its scratch buffers) per thread, since a [`Matcher`]
  /// carries a large heap slab and is meant to be reused.
  static ENGINE: RefCell<Engine> = RefCell::new(Engine::new());
}

/// The reusable matcher, the pattern it is currently matching, and the buffer
/// that turns a `&str` into the crate's UTF-32 view without reallocating.
struct Engine {
  pattern: Pattern,
  matcher: Matcher,
  buf:     Vec<char>,
}

impl Engine {
  fn new() -> Self {
    Self {
      pattern: Pattern::new(
        "",
        CaseMatching::Ignore,
        Normalization::Smart,
        nucleo_matcher::pattern::AtomKind::Fuzzy,
      ),
      matcher: Matcher::new(Config::DEFAULT),
      buf:     Vec::new(),
    }
  }

  /// Reparse this engine's pattern from `needle`; an empty (or blank) needle
  /// yields no atoms, which matches everything equally.
  fn reparse(&mut self, needle: &str) {
    self
      .pattern
      .reparse(needle, CaseMatching::Ignore, Normalization::Smart);
  }

  /// How well `hay` matches the current pattern, or `None` if it does not.
  fn score(&mut self, hay: &str) -> Option<u32> {
    let Engine {
      pattern,
      matcher,
      buf,
    } = self;
    pattern.score(Utf32Str::new(hay, buf), matcher)
  }
}

/// Run `f` with the thread's engine, borrowing it for the call.
fn engine<R>(f: impl FnOnce(&mut Engine) -> R) -> R {
  ENGINE.with(|cell| f(&mut cell.borrow_mut()))
}

/// How well `hay` matches `needle`, higher being better, or `None` if it
/// does not match. Case-insensitive and fuzzy. An empty needle matches
/// everything equally.
pub fn score(needle: &str, hay: &str) -> Option<u32> {
  engine(|engine| {
    engine.reparse(needle);
    engine.score(hay)
  })
}

/// The `items` that match `needle`, best match first, with `tie` breaking
/// ties. `hay` is the text of an item to match against.
pub fn rank<T>(
  needle: &str,
  items: impl IntoIterator<Item = T>,
  hay: impl Fn(&T) -> &str,
  tie: impl Fn(&T, &T) -> Ordering,
) -> Vec<T> {
  engine(|engine| {
    engine.reparse(needle);
    let mut scored: Vec<(u32, T)> = items
      .into_iter()
      .filter_map(|t| Some((engine.score(hay(&t))?, t)))
      .collect();
    scored
      .sort_by(|a, b| b.0.cmp(&a.0).then_with(|| tie(&a.1, &b.1)));
    scored.into_iter().map(|(_, t)| t).collect()
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn changing_the_text_resets_the_highlight() {
    let mut q = Query::default();
    q.set_text("ship".into());
    q.move_highlight(2, 5);
    q.set_text("ship".into());
    assert_eq!(q.highlight, 2, "same text, same highlight");
    q.set_text("ship v1".into());
    assert_eq!(q.highlight, 0);
  }

  #[test]
  fn the_highlight_stops_at_the_ends() {
    let mut q = Query::default();
    q.move_highlight(-1, 3);
    assert_eq!(q.highlight, 0);
    q.move_highlight(5, 3);
    assert_eq!(q.highlight, 2);
    q.move_highlight(1, 0);
    assert_eq!(q.highlight, 0);
    // A list that shrank under the highlight clamps it.
    let q = Query {
      text:      String::new(),
      highlight: 7,
    };
    assert_eq!(q.highlighted(3), 2);
  }

  #[test]
  fn an_empty_needle_matches_everything_equally() {
    assert_eq!(score("", "anything"), Some(0));
    assert_eq!(score("   ", "anything"), Some(0));
  }

  #[test]
  fn a_scattered_match_is_worse_than_contiguous() {
    let s = |needle, hay| score(needle, hay);
    assert_eq!(s("x", "Build backend"), None, "a miss is no match");
    assert!(s("BACK", "build backend").is_some(), "case-insensitive");
    // "backend" is contiguous and at a word boundary; "bbd" is scattered.
    assert!(s("backend", "Build backend") > s("bbd", "Build backend"));
  }

  #[test]
  fn rank_orders_best_first_and_keeps_ties_in_input_order() {
    let items = ["alpha", "beta", "gamma"];
    // An empty needle scores all equally, so the input order survives.
    assert_eq!(rank("", items, |s| s, Ord::cmp), items);
    // Only matching items come back; "eta" is a scattered match for beta
    // alone, and it leads.
    assert_eq!(rank("eta", items, |s| s, Ord::cmp), ["beta"]);
  }
}
