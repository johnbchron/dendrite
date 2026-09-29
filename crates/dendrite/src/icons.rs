//! Icons: a subset of Lucide (ISC licence, see `assets/Lucide-LICENSE.txt`)
//! bundled as a font, so an icon is a one-glyph label in the icon family.
//!
//! The font is built by `scripts/subset-icons.sh`, which lists every glyph
//! it keeps. To add an icon, add its Lucide name there and rerun the script,
//! then add a variant here with its codepoint (from Lucide's
//! `codepoints.json`). The test at the bottom fails if a variant's glyph is
//! missing from the bundled font.

use std::borrow::Cow;

use app::scene::Category;
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
  /// Go to a node.
  ArrowRight = 0xe049,
  /// A condition on a start or end date.
  Clock = 0xe087,
  /// A condition on a schedule's windows.
  CalendarRange = 0xe2bd,
  /// A condition on free time.
  Hourglass = 0xe296,
  /// A condition on a place.
  MapPin = 0xe111,
  /// A condition on a context.
  Tag = 0xe17f,
  /// A condition on a resource.
  Wallet = 0xe204,
  /// A satisfied requirement; the active choice in a list.
  Check = 0xe06c,
  /// Opens a menu; collapses a tray.
  ChevronDown = 0xe06d,
  /// A cycle, or anything else wrong.
  CircleAlert = 0xe077,
  /// A command, in the palette.
  Command = 0xe09a,
  /// Expands a tray.
  ChevronUp = 0xe070,
  /// Fit the graph to the window.
  Fit = 0xe257,
  /// The quest lens.
  Flag = 0xe0d1,
  /// Link mode; a requirement to add.
  Link = 0xe102,
  /// A condition (the nearest shape to its chamfered box).
  Octagon = 0xe126,
  /// Create something.
  Plus = 0xe13d,
  /// Redo.
  Redo = 0xe2a0,
  /// Search boxes.
  Search = 0xe151,
  /// Settings.
  Settings = 0xe154,
  /// A task; an unsatisfied requirement.
  Square = 0xe167,
  /// Delete.
  Trash = 0xe18e,
  /// Undo.
  Undo = 0xe2a1,
  /// Remove or close.
  X = 0xe1b2,
  /// Actionable work: the Now tray.
  Zap = 0xe1b4,
  /// Zoom in.
  ZoomIn = 0xe1b6,
  /// Zoom out.
  ZoomOut = 0xe1b7,
}

impl Icon {
  /// The glyph that marks a formula condition about `category`.
  pub fn for_category(category: Category) -> Self {
    match category {
      Category::Date => Icon::Clock,
      Category::Window => Icon::CalendarRange,
      Category::FreeTime => Icon::Hourglass,
      Category::Place => Icon::MapPin,
      Category::Resource => Icon::Wallet,
      Category::Context => Icon::Tag,
    }
  }
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
