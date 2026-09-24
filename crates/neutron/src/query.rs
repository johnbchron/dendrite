//! A typed query over a list: the text, which result is highlighted, and
//! how well an item matches.
//!
//! The quest switcher (and the command palette) take their keystrokes from
//! the key map rather than from a focused text field, because a text field
//! consumes the arrow keys that move the highlight. So the query's editing
//! lives here, as plain state the key map's commands act on.

/// An edit to a query's text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryEdit {
  /// Append typed or pasted text.
  Insert(String),
  /// Delete the last character.
  Backspace,
  /// Delete back to the start of the last word.
  DeleteWord,
}

/// A query's text and the highlighted result.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Query {
  /// What has been typed.
  pub text:      String,
  /// Index of the highlighted result.
  pub highlight: usize,
}

impl Query {
  /// Apply an edit. Any change to the text puts the highlight back on the
  /// first (best) result.
  pub fn edit(&mut self, edit: &QueryEdit) {
    match edit {
      // Control characters (a stray Ctrl chord) are not text.
      QueryEdit::Insert(s) => {
        self.text.extend(s.chars().filter(|c| !c.is_control()));
      }
      QueryEdit::Backspace => {
        self.text.pop();
      }
      QueryEdit::DeleteWord => {
        let trimmed = self.text.trim_end().len();
        let start = self.text[..trimmed]
          .rfind(char::is_whitespace)
          .map_or(0, |i| i + 1);
        self.text.truncate(start);
      }
    }
    self.highlight = 0;
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

/// How well `hay` matches `needle`, lower being better, or `None` if it does
/// not match. Case-insensitive. A substring beats a scattered match, and an
/// earlier substring beats a later one; a word-start substring beats one
/// inside a word. An empty needle matches everything equally.
pub fn score(needle: &str, hay: &str) -> Option<u32> {
  let needle = needle.trim().to_lowercase();
  if needle.is_empty() {
    return Some(0);
  }
  let hay = hay.to_lowercase();
  if let Some(pos) = hay.find(&needle) {
    let word_start =
      pos == 0 || hay[..pos].ends_with(|c: char| !c.is_alphanumeric());
    return Some(if word_start { 0 } else { 100 } + pos as u32);
  }
  // Every needle character in order, anywhere: scored by how spread out.
  let mut chars = hay.char_indices();
  let mut first = None;
  let mut last = 0;
  for n in needle.chars() {
    let (i, _) = chars.find(|&(_, h)| h == n)?;
    first.get_or_insert(i);
    last = i;
  }
  Some(1000 + (last - first.unwrap_or(0)) as u32)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn editing_resets_the_highlight() {
    let mut q = Query::default();
    q.edit(&QueryEdit::Insert("ship v".into()));
    q.move_highlight(2, 5);
    assert_eq!(q.highlight, 2);
    q.edit(&QueryEdit::Insert("1".into()));
    assert_eq!(q.text, "ship v1");
    assert_eq!(q.highlight, 0);
    q.edit(&QueryEdit::Backspace);
    assert_eq!(q.text, "ship v");
    q.edit(&QueryEdit::DeleteWord);
    assert_eq!(q.text, "ship ");
    q.edit(&QueryEdit::DeleteWord);
    assert_eq!(q.text, "");
    // Deleting from nothing is harmless.
    q.edit(&QueryEdit::Backspace);
    q.edit(&QueryEdit::DeleteWord);
    assert_eq!(q.text, "");
  }

  #[test]
  fn control_characters_are_not_inserted() {
    let mut q = Query::default();
    q.edit(&QueryEdit::Insert("a\u{1a}b".into()));
    assert_eq!(q.text, "ab");
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
  fn substrings_beat_scattered_matches_and_word_starts_win() {
    let s = |needle, hay| score(needle, hay);
    assert_eq!(s("", "anything"), Some(0));
    assert_eq!(s("x", "Build backend"), None);
    // Word starts, earliest first.
    assert!(s("back", "Build backend") < s("end", "Build backend"));
    assert!(s("bu", "Build backend") < s("ba", "Build backend"));
    // Inside a word is worse than any word start.
    assert!(s("ack", "Build backend") > s("backend", "Build backend"));
    // A scattered match still matches, below any substring.
    let scattered = s("bbd", "Build backend").unwrap();
    assert!(scattered >= 1000);
    assert!(s("BACK", "build backend").is_some(), "case-insensitive");
  }
}
