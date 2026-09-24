//! The app's typeface.
//!
//! Neither of Masonry's text defaults gives a deliberate face. Its own widgets
//! ask for `GenericFamily::SystemUi`, which fontconfig commonly resolves to
//! DejaVu Sans, and a xilem `label` goes further still: it overwrites that
//! default with an *empty* font stack, leaving parley to fall back to whatever
//! the script fallback chain offers. So the app ships its own face — Inter,
//! under the SIL Open Font License (see `assets/Inter-LICENSE.txt`) — and names
//! it everywhere text is shaped:
//!
//! - [`DATA`] is registered with the Xilem app at startup;
//! - [`STACK`] goes on every label, and onto the canvas's hand-shaped text;
//! - [`install`] remaps `SystemUi` to the bundled face, which is the only way
//!   to reach `text_input`, since xilem 0.4 gives it no font setter.

use std::borrow::Cow;

use masonry::parley::{
  FontContext,
  fontique::GenericFamily,
  style::{FontFamily, FontStack},
};

/// The bundled variable font; one file covers every weight.
pub const DATA: &[u8] = include_bytes!("../assets/InterVariable.ttf");

/// The family name [`DATA`] registers under.
pub const FAMILY: &str = "Inter Variable";

/// The font stack every piece of app text uses: the bundled face, then the
/// system UI face for any glyph it lacks.
pub const STACK: FontStack<'static> = FontStack::List(Cow::Borrowed(&[
  FontFamily::Named(Cow::Borrowed(FAMILY)),
  FontFamily::Generic(GenericFamily::SystemUi),
]));

/// Point the `SystemUi` generic family at the bundled face, so widgets that
/// only ever ask for the default (text inputs) pick it up too.
///
/// Returns whether the face was found. It is registered by the Xilem driver
/// when the event loop starts, so callers should retry until this succeeds.
pub fn install(font_cx: &mut FontContext) -> bool {
  let Some(family) = font_cx.collection.family_by_name(FAMILY) else {
    return false;
  };
  font_cx.collection.set_generic_families(
    GenericFamily::SystemUi,
    std::iter::once(family.id()),
  );
  true
}
