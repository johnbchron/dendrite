//! A backend that keeps everything in memory and nothing anywhere else.
//!
//! For tests, and for a scratch session that is deliberately not saved. It
//! is the reference implementation of [`Backend`]: short enough to read in
//! one go when writing a real one.

use std::{
  collections::HashMap,
  sync::{Mutex, MutexGuard},
};

use base::Event;

use crate::{Backend, Error};

/// An event log and a preference map, both held in memory.
#[derive(Debug, Default)]
pub struct Memory {
  /// The log. A position is an index into this, one-based, so 0 can mean
  /// "empty log" as it does for a real store.
  events:   Vec<Event>,
  /// Behind a lock because [`Backend::set_setting`] takes `&self`, as a
  /// real store's does: it writes through a connection it shares.
  settings: Mutex<HashMap<String, String>>,
}

impl Memory {
  /// The preference map, or an error if a panic poisoned it.
  fn settings(&self) -> Result<MutexGuard<'_, HashMap<String, String>>, Error> {
    self
      .settings
      .lock()
      .map_err(|_| "settings lock poisoned".into())
  }
}

impl Backend for Memory {
  fn replay(&self) -> Result<(Vec<Event>, i64), Error> {
    Ok((self.events.clone(), self.events.len() as i64))
  }

  fn event_count(&self) -> Result<u64, Error> { Ok(self.events.len() as u64) }

  fn append(&mut self, events: &[Event]) -> Result<i64, Error> {
    self.events.extend_from_slice(events);
    Ok(self.events.len() as i64)
  }

  fn replace(&mut self, at: i64, event: &Event) -> Result<(), Error> {
    if let Some(slot) = self.events.get_mut(at as usize - 1) {
      *slot = event.clone();
    }
    Ok(())
  }

  fn setting(&self, key: &str) -> Result<Option<String>, Error> {
    Ok(self.settings()?.get(key).cloned())
  }

  fn set_setting(&self, key: &str, value: &str) -> Result<(), Error> {
    self.settings()?.insert(key.to_string(), value.to_string());
    Ok(())
  }
}
