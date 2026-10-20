//! Typed identifiers.

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Wraps a raw id. Validation of the format belongs to the caller that creates ids.
            pub fn new(raw: impl Into<String>) -> Self {
                Self(raw.into())
            }

            /// The raw id.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
    };
}

id_type!(
    /// A project (a fork the user customizes) known to the agent.
    ProjectId
);
id_type!(
    /// One task sent by the user (may be split into steps).
    TaskId
);
id_type!(
    /// A customization recorded in `CUSTOMIZATIONS.md` and in commit trailers.
    CustomizationId
);
id_type!(
    /// A paired browser key (hash of its public key).
    KeyId
);
