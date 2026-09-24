# Vendored crates

## `masonry` (0.4.0, patched)

An unmodified copy of `masonry` 0.4.0 from crates.io with
[`masonry.patch`](./masonry.patch) applied, wired in through
`[patch.crates-io]` in the workspace `Cargo.toml`. The patch changes
`TextArea`'s key handling in two ways:

- **Command-key chords are not text.** Upstream inserts the plain letter for
  Ctrl+K (Cmd+K on macOS), so shortcuts pressed in a text field typed into it.
  The patched field leaves such chords unhandled, so they reach the app's key
  map (`crates/neutron/src/keymap.rs`). Ctrl+A/C/X/V keep their usual meaning.
- **Up and Down pass through single-line text.** With no line to move to,
  they are left unhandled, so a search field can move the highlight of the
  list under it.

It also allows `unfulfilled_lint_expectations` in the crate's `Cargo.toml`:
crates.io builds cap a dependency's lints, a path dependency's are not, and
newer rustc flags one expectation in masonry's docs.

To upgrade masonry: copy the new release here, re-apply the patch
(`patch -p1 -d vendor/masonry < vendor/masonry.patch`), and check whether
upstream has fixed either behaviour, in which case drop that hunk.
