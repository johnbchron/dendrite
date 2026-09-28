//! `widgets` — the Masonry widgets and Xilem views Neutron needed and the
//! toolkit does not ship.
//!
//! Nothing here knows anything about Neutron. Each widget takes the colours,
//! sizes and font it should use, so the palette and the design tokens stay
//! with the app; `neutron::themed` is where this app binds them.
//!
//! - [`appear`]: a surface that eases the last few pixels into place.
//! - [`divider`]: a draggable split, reporting travel from a fixed anchor.
//! - [`field`]: a single-line text field with a frame and focus reporting.
//! - [`hover_row`]: a list row whose trailing control shows on hover.
//! - [`surface`]: a ground with a border and a shadow, for floating chrome.
//! - [`timer`]: a keyed one-shot timer as a view.
//! - [`tooltip`]: a label that appears under a control after a pause.

pub mod appear;
pub mod divider;
pub mod field;
pub mod hover_row;
pub mod surface;
pub mod timer;
pub mod tooltip;
