//! Commands from the key map.

use super::AppState;
use crate::keymap::{Command, Flags};

impl AppState {
  /// What the key map needs to know to resolve a key.
  pub fn key_flags(&self) -> Flags {
    Flags {
      selection: self.selected.is_some(),
      query:     self.picker_open() || self.palette_open(),
    }
  }

  /// Run a command from the key map.
  pub fn run(&mut self, command: Command) {
    match command {
      Command::Undo => self.undo(),
      Command::Redo => self.redo(),
      Command::Delete => self.delete_selected(),
      Command::Escape => self.escape(),
      // The palette sits above the quest switcher, so it takes the query
      // keys when both could.
      Command::Move(by) => {
        if self.palette_open() {
          let len = self.palette_rows().0.len();
          self.palette_query.move_highlight(by, len);
        } else if self.picker_open() {
          let len = self.quest_rows().len();
          self.quest_query.move_highlight(by, len);
        }
      }
      Command::Accept => {
        if self.palette_open() {
          self.accept_palette();
        } else if self.picker_open() {
          self.accept_quest();
        }
      }
      Command::New { condition } => {
        if condition {
          self.add_condition();
        } else {
          self.add_task();
        }
      }
      Command::Primary => self.toggle_selected(),
      Command::Nav(direction) => self.navigate(direction),
      Command::Zoom(step) => self.zoom(step),
      Command::Fit => self.recenter(),
      Command::Quests => self.toggle_picker(),
      Command::Now => self.toggle_now(),
      Command::Palette { nodes_only } => {
        if self.palette_open() {
          self.close_popovers();
        } else {
          self.open_palette(nodes_only);
        }
      }
    }
  }

  /// Back out one step: cancel link mode, else close an open popover, else
  /// clear the selection.
  pub fn escape(&mut self) {
    if self.linking {
      self.cancel_link();
    } else if self.popover_open() {
      self.close_popovers();
    } else {
      self.select(None);
    }
  }
}
