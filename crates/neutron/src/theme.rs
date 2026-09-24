//! The colour palettes the UI can be set to (PLAN §5).
//!
//! Both painted surfaces — the Vello canvas and the Xilem chrome — read their
//! colours from one [`Theme`], so a palette is a single value the app carries
//! rather than two sets of constants that can drift apart. Every
//! theme fills the same slots, and the meanings are fixed across all of
//! them: `accent` is selection, green is satisfied, red is a cycle, grey is
//! waiting. The accent is always a different hue from the Ready border, so a
//! selected Ready node is told apart from its unselected neighbours by colour
//! and not just stroke width.
//!
//! The chosen theme is a *preference*, not graph data: it is stored in the
//! `meta` table rather than the event log, so switching palettes never lands
//! on the undo stack.

use base::NodeState;
use masonry::peniko::Color;

/// One complete palette: canvas colours, per-state node colours, and the
/// side panel's chrome.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Theme {
  /// Stable key used to persist the choice. Never shown to the user.
  pub id:   &'static str,
  /// Display name, shown in the palette picker.
  pub name: &'static str,

  /// Canvas ground.
  pub bg:     Color,
  /// Ordinary edges and their arrowheads.
  pub edge:   Color,
  /// Edges the cycle-cut reversed.
  pub cycle:  Color,
  /// Node labels on the canvas, and body text in the panel.
  pub text:   Color,
  /// Selection, the Ready state, and interactive hints.
  pub accent: Color,

  /// `(fill, border)` for a completed task or satisfied condition.
  pub done:    (Color, Color),
  /// `(fill, border)` for a Ready node.
  pub ready:   (Color, Color),
  /// `(fill, border)` for a Blocked node.
  pub blocked: (Color, Color),
  /// `(fill, border)` for a Pending condition.
  pub pending: (Color, Color),
  /// `(fill, border)` for a node caught in a cycle.
  pub cyclic:  (Color, Color),

  /// Fields, framed button groups and other insets on a surface.
  pub sunken: Color,
  /// Hairlines.
  pub rule:   Color,
  /// Secondary text.
  pub muted:  Color,

  /// Cards and bars that sit over the canvas: the top bar, the inspector,
  /// the Now tray.
  pub surface:        Color,
  /// Popovers and the command palette, one step above `surface`.
  pub surface_raised: Color,
  /// Translucent wash over the canvas behind a modal overlay.
  pub scrim:          Color,
  /// Keyboard-focus ring on focusable controls.
  pub focus:          Color,
  /// Drop shadow under floating surfaces (usually translucent).
  pub shadow:         Color,
  /// Text and icons drawn on an `accent` fill (the primary button).
  pub on_accent:      Color,

  /// Chip ground for Ready.
  pub chip_ready:   Color,
  /// Chip ground for completed/satisfied.
  pub chip_done:    Color,
  /// Chip ground for blocked/pending.
  pub chip_blocked: Color,
  /// Chip ground for cyclic.
  pub chip_cyclic:  Color,
}

impl Theme {
  /// `(fill, border)` for a node in the given derived state.
  pub fn for_state(&self, state: NodeState) -> (Color, Color) {
    match state {
      NodeState::Completed | NodeState::Satisfied => self.done,
      NodeState::Ready => self.ready,
      NodeState::Blocked => self.blocked,
      NodeState::Pending => self.pending,
      NodeState::Cyclic => self.cyclic,
    }
  }

  /// Chip ground for a node in the given derived state.
  pub fn chip_for_state(&self, state: NodeState) -> Color {
    match state {
      NodeState::Completed | NodeState::Satisfied => self.chip_done,
      NodeState::Ready => self.chip_ready,
      NodeState::Cyclic => self.chip_cyclic,
      NodeState::Blocked | NodeState::Pending => self.chip_blocked,
    }
  }
}

/// Dim a fill for pulled-in (unclaimed) nodes in a quest scope. Alpha rather
/// than a fixed colour, so every palette dims toward its own ground.
pub fn dim(c: Color) -> Color { c.multiply_alpha(0.5) }

/// The original palette: cool blue-grey ground, bright blue for Ready and a
/// warm gold for selection.
pub const SLATE: Theme = Theme {
  id:   "slate",
  name: "Slate",

  bg:     Color::from_rgb8(24, 26, 32),
  edge:   Color::from_rgb8(96, 104, 120),
  cycle:  Color::from_rgb8(214, 105, 90),
  text:   Color::from_rgb8(232, 236, 244),
  accent: Color::from_rgb8(236, 186, 92),

  done:    (Color::from_rgb8(40, 66, 48), Color::from_rgb8(96, 170, 116)),
  ready:   (Color::from_rgb8(40, 58, 82), Color::from_rgb8(94, 168, 246)),
  blocked: (Color::from_rgb8(48, 52, 62), Color::from_rgb8(96, 104, 120)),
  pending: (
    Color::from_rgb8(48, 52, 62),
    Color::from_rgb8(150, 156, 168),
  ),
  cyclic:  (Color::from_rgb8(72, 44, 42), Color::from_rgb8(214, 105, 90)),

  sunken: Color::from_rgb8(30, 33, 41),
  rule:   Color::from_rgb8(46, 50, 60),
  muted:  Color::from_rgb8(122, 131, 146),

  surface:        Color::from_rgb8(30, 33, 41),
  surface_raised: Color::from_rgb8(38, 42, 52),
  scrim:          Color::from_rgba8(8, 10, 14, 140),
  focus:          Color::from_rgb8(236, 186, 92),
  shadow:         Color::from_rgba8(0, 0, 0, 115),
  on_accent:      Color::from_rgb8(28, 22, 10),

  chip_ready:   Color::from_rgb8(40, 74, 118),
  chip_done:    Color::from_rgb8(40, 82, 56),
  chip_blocked: Color::from_rgb8(60, 64, 76),
  chip_cyclic:  Color::from_rgb8(120, 54, 50),
};

/// Near-black ground: teal marks Ready, violet marks the selection.
pub const GRAPHITE: Theme = Theme {
  id:   "graphite",
  name: "Graphite",

  bg:     Color::from_rgb8(15, 17, 20),
  edge:   Color::from_rgb8(86, 95, 107),
  cycle:  Color::from_rgb8(224, 112, 90),
  text:   Color::from_rgb8(234, 238, 243),
  accent: Color::from_rgb8(178, 140, 240),

  done:    (Color::from_rgb8(16, 50, 42), Color::from_rgb8(79, 176, 138)),
  ready:   (Color::from_rgb8(15, 50, 58), Color::from_rgb8(69, 184, 194)),
  blocked: (Color::from_rgb8(27, 31, 37), Color::from_rgb8(86, 95, 107)),
  pending: (
    Color::from_rgb8(27, 31, 37),
    Color::from_rgb8(139, 149, 161),
  ),
  cyclic:  (Color::from_rgb8(52, 31, 28), Color::from_rgb8(224, 112, 90)),

  sunken: Color::from_rgb8(24, 28, 34),
  rule:   Color::from_rgb8(38, 43, 51),
  muted:  Color::from_rgb8(121, 130, 142),

  surface:        Color::from_rgb8(22, 25, 30),
  surface_raised: Color::from_rgb8(30, 34, 41),
  scrim:          Color::from_rgba8(4, 5, 7, 150),
  focus:          Color::from_rgb8(178, 140, 240),
  shadow:         Color::from_rgba8(0, 0, 0, 128),
  on_accent:      Color::from_rgb8(20, 14, 32),

  chip_ready:   Color::from_rgb8(18, 64, 73),
  chip_done:    Color::from_rgb8(20, 69, 58),
  chip_blocked: Color::from_rgb8(42, 47, 55),
  chip_cyclic:  Color::from_rgb8(74, 32, 25),
};

/// A warm, low-glare ground for long sessions: amber marks Ready, sage marks
/// done, a cool teal marks the selection, and the red cycle edges read as an
/// alarm against the brown.
pub const UMBER: Theme = Theme {
  id:   "umber",
  name: "Umber",

  bg:     Color::from_rgb8(26, 23, 20),
  edge:   Color::from_rgb8(110, 98, 85),
  cycle:  Color::from_rgb8(210, 112, 90),
  text:   Color::from_rgb8(241, 234, 224),
  accent: Color::from_rgb8(112, 190, 192),

  done:    (
    Color::from_rgb8(41, 50, 30),
    Color::from_rgb8(143, 174, 114),
  ),
  ready:   (Color::from_rgb8(58, 44, 24), Color::from_rgb8(224, 167, 94)),
  blocked: (Color::from_rgb8(38, 33, 25), Color::from_rgb8(110, 98, 85)),
  pending: (
    Color::from_rgb8(38, 33, 25),
    Color::from_rgb8(162, 149, 138),
  ),
  cyclic:  (Color::from_rgb8(58, 35, 29), Color::from_rgb8(210, 112, 90)),

  sunken: Color::from_rgb8(37, 31, 26),
  rule:   Color::from_rgb8(58, 50, 42),
  muted:  Color::from_rgb8(155, 142, 127),

  surface:        Color::from_rgb8(36, 31, 26),
  surface_raised: Color::from_rgb8(46, 40, 33),
  scrim:          Color::from_rgba8(10, 8, 6, 140),
  focus:          Color::from_rgb8(112, 190, 192),
  shadow:         Color::from_rgba8(0, 0, 0, 128),
  on_accent:      Color::from_rgb8(14, 28, 28),

  chip_ready:   Color::from_rgb8(74, 54, 24),
  chip_done:    Color::from_rgb8(51, 64, 31),
  chip_blocked: Color::from_rgb8(51, 44, 36),
  chip_cyclic:  Color::from_rgb8(78, 42, 32),
};

/// Every ground a step lighter and less saturated, so a graph that is mostly
/// Blocked stops reading as a wall of black. Soft blue marks Ready, soft gold
/// the selection.
pub const FROST: Theme = Theme {
  id:   "frost",
  name: "Frost",

  bg:     Color::from_rgb8(35, 41, 53),
  edge:   Color::from_rgb8(107, 118, 136),
  cycle:  Color::from_rgb8(206, 138, 133),
  text:   Color::from_rgb8(228, 233, 240),
  accent: Color::from_rgb8(226, 190, 128),

  done:    (
    Color::from_rgb8(46, 64, 56),
    Color::from_rgb8(147, 191, 160),
  ),
  ready:   (
    Color::from_rgb8(44, 58, 78),
    Color::from_rgb8(134, 169, 214),
  ),
  blocked: (
    Color::from_rgb8(46, 53, 66),
    Color::from_rgb8(107, 118, 136),
  ),
  pending: (
    Color::from_rgb8(46, 53, 66),
    Color::from_rgb8(154, 164, 180),
  ),
  cyclic:  (
    Color::from_rgb8(67, 48, 47),
    Color::from_rgb8(206, 138, 133),
  ),

  sunken: Color::from_rgb8(48, 56, 69),
  rule:   Color::from_rgb8(62, 72, 89),
  muted:  Color::from_rgb8(141, 151, 168),

  surface:        Color::from_rgb8(44, 51, 64),
  surface_raised: Color::from_rgb8(54, 62, 77),
  scrim:          Color::from_rgba8(14, 17, 23, 128),
  focus:          Color::from_rgb8(226, 190, 128),
  shadow:         Color::from_rgba8(0, 0, 0, 90),
  on_accent:      Color::from_rgb8(36, 28, 12),

  chip_ready:   Color::from_rgb8(53, 73, 106),
  chip_done:    Color::from_rgb8(55, 84, 74),
  chip_blocked: Color::from_rgb8(58, 67, 81),
  chip_cyclic:  Color::from_rgb8(90, 59, 57),
};

/// After [Evergarden](https://evergarden.moe): pastels on a green-grey
/// forest floor. Its green marks done, its blue Ready and its cherry pink the
/// selection; fills are the same pastels washed thinly over the ground.
pub const EVERGARDEN: Theme = Theme {
  id:   "evergarden",
  name: "Evergarden",

  bg:     Color::from_rgb8(30, 37, 40),
  edge:   Color::from_rgb8(88, 104, 109),
  cycle:  Color::from_rgb8(245, 127, 130),
  text:   Color::from_rgb8(248, 249, 232),
  accent: Color::from_rgb8(243, 192, 229),

  done:    (
    Color::from_rgb8(65, 75, 68),
    Color::from_rgb8(203, 227, 179),
  ),
  ready:   (
    Color::from_rgb8(60, 70, 79),
    Color::from_rgb8(178, 202, 237),
  ),
  blocked: (Color::from_rgb8(38, 47, 51), Color::from_rgb8(88, 104, 109)),
  pending: (
    Color::from_rgb8(38, 47, 51),
    Color::from_rgb8(150, 180, 170),
  ),
  cyclic:  (
    Color::from_rgb8(73, 55, 58),
    Color::from_rgb8(245, 127, 130),
  ),

  sunken: Color::from_rgb8(30, 37, 40),
  rule:   Color::from_rgb8(55, 65, 69),
  muted:  Color::from_rgb8(131, 158, 154),

  surface:        Color::from_rgb8(38, 47, 51),
  surface_raised: Color::from_rgb8(46, 56, 60),
  scrim:          Color::from_rgba8(13, 16, 18, 140),
  focus:          Color::from_rgb8(243, 192, 229),
  shadow:         Color::from_rgba8(0, 0, 0, 128),
  on_accent:      Color::from_rgb8(23, 28, 31),

  chip_ready:   Color::from_rgb8(77, 90, 103),
  chip_done:    Color::from_rgb8(85, 98, 84),
  chip_blocked: Color::from_rgb8(55, 65, 69),
  chip_cyclic:  Color::from_rgb8(99, 66, 69),
};

/// The daylight inverse: paper ground, ink text, tinted fills and saturated
/// borders doing the state work — blue for Ready, purple for the selection.
/// Blocked sits a shade below the ground, Pending a shade above it, so the
/// two waiting states stay apart.
pub const MERIDIAN: Theme = Theme {
  id:   "meridian",
  name: "Meridian",

  bg:     Color::from_rgb8(236, 240, 243),
  edge:   Color::from_rgb8(128, 139, 154),
  cycle:  Color::from_rgb8(192, 71, 58),
  text:   Color::from_rgb8(27, 32, 41),
  accent: Color::from_rgb8(126, 70, 196),

  done:    (
    Color::from_rgb8(208, 235, 218),
    Color::from_rgb8(38, 131, 86),
  ),
  ready:   (
    Color::from_rgb8(208, 226, 251),
    Color::from_rgb8(38, 102, 204),
  ),
  blocked: (
    Color::from_rgb8(221, 226, 232),
    Color::from_rgb8(134, 144, 160),
  ),
  pending: (
    Color::from_rgb8(239, 242, 245),
    Color::from_rgb8(97, 109, 125),
  ),
  cyclic:  (
    Color::from_rgb8(250, 215, 208),
    Color::from_rgb8(189, 62, 49),
  ),

  sunken: Color::from_rgb8(240, 243, 246),
  rule:   Color::from_rgb8(210, 217, 226),
  muted:  Color::from_rgb8(94, 106, 121),

  surface:        Color::from_rgb8(255, 255, 255),
  surface_raised: Color::from_rgb8(255, 255, 255),
  scrim:          Color::from_rgba8(20, 26, 36, 72),
  focus:          Color::from_rgb8(126, 70, 196),
  shadow:         Color::from_rgba8(24, 32, 48, 46),
  on_accent:      Color::from_rgb8(255, 255, 255),

  chip_ready:   Color::from_rgb8(198, 218, 248),
  chip_done:    Color::from_rgb8(199, 232, 211),
  chip_blocked: Color::from_rgb8(216, 222, 230),
  chip_cyclic:  Color::from_rgb8(248, 206, 198),
};

/// Every palette the picker offers, in the order it shows them.
pub const ALL: &[&Theme] =
  &[&SLATE, &GRAPHITE, &UMBER, &FROST, &EVERGARDEN, &MERIDIAN];

/// The palette used when nothing has been chosen (or a stored choice no
/// longer exists).
pub const DEFAULT: &Theme = &SLATE;

/// Look a palette up by its persisted [`Theme::id`].
pub fn by_id(id: &str) -> Option<&'static Theme> {
  ALL.iter().copied().find(|t| t.id == id)
}

#[cfg(test)]
mod tests {
  use super::*;

  /// Selection is drawn in `accent` over a node's state border, so if the two
  /// match, a selected Ready node differs from its neighbours only in stroke
  /// width.
  #[test]
  fn selection_never_matches_the_ready_border() {
    for theme in ALL {
      assert_ne!(
        theme.accent, theme.ready.1,
        "{} selects in its Ready colour",
        theme.name
      );
    }
  }

  /// WCAG relative luminance of an opaque colour.
  fn luminance(c: Color) -> f64 {
    let [r, g, b, _] = c.components;
    let lin = |v: f32| {
      let v = f64::from(v);
      if v <= 0.040_45 {
        v / 12.92
      } else {
        ((v + 0.055) / 1.055).powf(2.4)
      }
    };
    0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b)
  }

  /// WCAG contrast ratio between two opaque colours.
  fn contrast(a: Color, b: Color) -> f64 {
    let (la, lb) = (luminance(a), luminance(b));
    (la.max(lb) + 0.05) / (la.min(lb) + 0.05)
  }

  /// The chrome's text stays readable on every surface it sits on: body text
  /// at WCAG AA (4.5:1), secondary text at the large/UI level (3:1), and the
  /// primary button's label on its accent fill at AA.
  #[test]
  fn chrome_text_is_readable_on_its_surfaces() {
    for t in ALL {
      for (ground, name) in
        [(t.surface, "surface"), (t.surface_raised, "surface_raised")]
      {
        let body = contrast(t.text, ground);
        assert!(body >= 4.5, "{}: text on {name} is {body:.2}", t.name);
        let muted = contrast(t.muted, ground);
        assert!(muted >= 3.0, "{}: muted on {name} is {muted:.2}", t.name);
      }
      let primary = contrast(t.on_accent, t.accent);
      assert!(primary >= 4.5, "{}: on_accent is {primary:.2}", t.name);
    }
  }
}
