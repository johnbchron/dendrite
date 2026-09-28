//! `app` — everything Neutron does, minus how it is drawn.
//!
//! The application's whole behaviour lives here: what is selected, what the
//! inspector says about it, link mode, quests and their lens, the command
//! palette, the Now tray, the key map, the colour palettes, and the scene
//! handed to whatever paints the canvas. It owns an editing
//! [`Session`](session::Session) and turns gestures into [`base::Event`]s.
//!
//! There is no UI toolkit here, and no platform. The only dependencies
//! beyond the pure crates are `kurbo`, `peniko` and `ui-events` — the
//! linebender vocabulary crates for geometry, colour and input, which carry
//! no windowing, GPU or OS code of their own. A port to another platform
//! reuses this crate whole and writes only views.
//!
//! - [`state`]: the application state and the commands that change it.
//! - [`scene`]: what the app hands a canvas to draw, and what it reports back.
//! - [`camera`]: what the app can ask of the canvas's camera.
//! - [`keymap`]: what a key means.
//! - [`focus`]: naming a text field so it can be focused without knowing what
//!   widget it is.
//! - [`query`]: typing into a list: the text, the highlight, and matching.
//! - [`theme`]: the colour palettes every painted surface reads from.
//! - [`tokens`]: the spacing, size and type scale the chrome is built on.

pub mod camera;
pub mod focus;
pub mod keymap;
pub mod query;
pub mod scene;
pub mod state;
pub mod theme;
pub mod tokens;
