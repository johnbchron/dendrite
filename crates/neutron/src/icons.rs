//! Icons: a subset of Lucide (ISC licence, see `assets/Lucide-LICENSE.txt`)
//! bundled as a font, so an icon is a one-glyph label in the icon family.
//!
//! The font is built by `scripts/subset-icons.sh`, which lists every glyph
//! it keeps. To add an icon, add its Lucide name there and rerun the script,
//! then add a variant here with its codepoint (from Lucide's
//! `codepoints.json`). The test at the bottom fails if a variant's glyph is
//! missing from the bundled font.

use std::borrow::Cow;

use masonry::{
  parley::style::{FontFamily, FontStack},
  peniko::Color,
};
use xilem::{
  WidgetView,
  style::Style as _,
  view::{Label, label},
};

/// The bundled icon font.
pub const DATA: &[u8] = include_bytes!("../assets/lucide-subset.ttf");

/// The family name [`DATA`] registers under.
pub const FAMILY: &str = "lucide";

/// Declare [`Icon`] from `Variant = codepoint` pairs, and the list the
/// font-coverage test walks.
macro_rules! icons {
  ($($(#[$doc:meta])* $name:ident = $code:literal,)+) => {
    /// An icon from the bundled set.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Icon {
      $($(#[$doc])* $name,)+
    }

    impl Icon {
      /// The glyph that draws this icon in the icon font.
      pub fn glyph(self) -> char {
        let code: u32 = match self {
          $(Icon::$name => $code,)+
        };
        char::from_u32(code).expect("icon codepoints are valid chars")
      }
    }

    #[cfg(test)]
    const ALL: &[Icon] = &[$(Icon::$name,)+];
  };
}

icons! {
  /// A satisfied requirement; the active choice in a list.
  Check = 0xe06c,
  /// Opens a menu.
  ChevronDown = 0xe06d,
  /// Fit the graph to the window.
  Fit = 0xe257,
  /// The quest lens.
  Flag = 0xe0d1,
  /// Create something.
  Plus = 0xe13d,
  /// Redo.
  Redo = 0xe2a0,
  /// Search boxes.
  Search = 0xe151,
  /// Settings.
  Settings = 0xe154,
  /// An unsatisfied requirement.
  Square = 0xe167,
  /// Undo.
  Undo = 0xe2a1,
  /// Remove or close.
  X = 0xe1b2,
  /// Zoom in.
  ZoomIn = 0xe1b6,
  /// Zoom out.
  ZoomOut = 0xe1b7,
}

/// An icon at `size` logical pixels, uncoloured: a plain [`Label`] for
/// callers that go on to style it (colours for each state, say).
pub fn icon_label(icon: Icon, size: f32) -> Label {
  label(String::from(icon.glyph()))
    .font(FontStack::Single(FontFamily::Named(Cow::Borrowed(FAMILY))))
    .text_size(size)
}

/// An icon at `size` logical pixels in `color`.
pub fn icon<State: 'static, Action: 'static>(
  icon: Icon,
  size: f32,
  color: Color,
) -> impl WidgetView<State, Action> + use<State, Action> {
  icon_label(icon, size).color(color)
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Every icon has a glyph in the bundled subset, and the font names itself
  /// the way [`icon`] asks for it.
  #[test]
  fn every_icon_is_in_the_bundled_font() {
    let face = ttf_parser::Face::parse(DATA, 0).expect("icon font parses");
    let family = face
      .names()
      .into_iter()
      .find(|n| n.name_id == ttf_parser::name_id::FAMILY)
      .and_then(|n| n.to_string())
      .expect("icon font has a family name");
    assert_eq!(family, FAMILY);
    for icon in ALL {
      assert!(
        face.glyph_index(icon.glyph()).is_some(),
        "{icon:?} ({:#x}) is missing from the font",
        u32::from(icon.glyph())
      );
    }
  }
}
