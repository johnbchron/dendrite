//! Strongly-typed ULID identifiers.
//!
//! Every entity in the graph carries a [`Ulid`]-backed id. ULIDs are
//! lexicographically sortable and globally unique without coordination,
//! which keeps the door open for CRDT-style sync later (see PLAN §1).

use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// Declare a newtype wrapper around [`Ulid`].
///
/// The wrappers are deliberately distinct types so a `NodeId` can never be
/// passed where an `EdgeId` is expected.
macro_rules! id_type {
  ($(#[$m:meta])* $name:ident) => {
    $(#[$m])*
    #[derive(
      Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
    )]
    #[serde(transparent)]
    pub struct $name(pub Ulid);

    impl $name {
      /// Mint a fresh id from the current time + randomness.
      pub fn new() -> Self {
        Self(Ulid::generate())
      }

      /// Reconstruct an id from its raw 128-bit value (round-trips storage).
      pub fn from_u128(v: u128) -> Self {
        Self(Ulid::from(v))
      }

      /// The raw 128-bit value, for compact storage.
      pub fn to_u128(self) -> u128 {
        self.0.into()
      }
    }

    impl Default for $name {
      fn default() -> Self {
        Self::new()
      }
    }

    impl core::fmt::Display for $name {
      fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
      }
    }

    impl core::fmt::Debug for $name {
      fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, concat!(stringify!($name), "({})"), self.0)
      }
    }

    impl core::str::FromStr for $name {
      type Err = ulid::DecodeError;

      fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(Self(Ulid::from_string(s)?))
      }
    }
  };
}

id_type!(
  /// Identifies a [`Node`](crate::Node) in the global graph.
  NodeId
);
id_type!(
  /// Identifies an [`Edge`](crate::Edge) in the global graph.
  EdgeId
);
id_type!(
  /// Identifies a [`Quest`](crate::Quest).
  QuestId
);
id_type!(
  /// Identifies a single [`Event`](crate::Event) in the append-only log.
  EventId
);
