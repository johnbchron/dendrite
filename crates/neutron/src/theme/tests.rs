use super::*;

/// Selection is drawn in `accent` over a node's state border, so if the two
/// match, a selected Ready node differs from its neighbours only in stroke
/// width.
#[test]
fn selection_never_matches_the_ready_border() {
  for theme in Theme::ALL {
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
  for t in Theme::ALL {
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
