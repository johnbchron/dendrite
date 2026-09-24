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
//! - [`STACK`] goes on every label, every text field (see [`crate::field`]) and
//!   the canvas's hand-shaped text.

use std::borrow::Cow;

use masonry::parley::{
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
