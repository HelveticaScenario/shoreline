//! The output device the user chose, and what polling the devices keeps for
//! it.

use std::time::{Duration, Instant};

/// How often the device list is read for a chosen output that is not
/// connected.
pub(crate) const MISSING_OUTPUT_CHECK: Duration = Duration::from_secs(3);

/// The output the user chose.
#[derive(Debug)]
pub(crate) enum OutputChoice {
    /// System Default: the streams follow the host's system default output.
    SystemDefault {
        /// The system default output last moved to, so a default that cannot
        /// be opened is not retried until it changes again.
        seen_default: Option<String>,
    },
    /// A device picked by id, which the streams are open on. It never follows
    /// the default, also while its device is gone.
    Device,
    /// A device picked by id that is not connected. Until it is listed, the
    /// streams play on the system default and follow it.
    Missing(MissingOutput),
}

/// The chosen output, while it is not connected.
#[derive(Debug)]
pub(crate) struct MissingOutput {
    pub(crate) id: String,
    pub(crate) next_check: Instant,
    /// A listed device that would not open was reported.
    pub(crate) error_reported: bool,
    /// See [`OutputChoice::SystemDefault`].
    seen_default: Option<String>,
}

impl OutputChoice {
    /// The choice of `asked` (`None` for System Default) once streams opened
    /// on `opened`, the device `asked` names or, when it is not connected,
    /// the system default.
    pub(crate) fn opened(asked: Option<String>, opened: &str) -> Self {
        let seen_default = Some(opened.to_string());
        match asked {
            None => Self::SystemDefault { seen_default },
            Some(id) if id == opened => Self::Device,
            Some(id) => Self::Missing(MissingOutput {
                id,
                next_check: Instant::now(),
                error_reported: false,
                seen_default,
            }),
        }
    }

    /// The id of the chosen device, `None` for System Default. `open_on` is
    /// the device the streams are open on.
    pub(crate) fn device_id(&self, open_on: &str) -> Option<String> {
        match self {
            Self::SystemDefault { .. } => None,
            Self::Device => Some(open_on.to_string()),
            Self::Missing(missing) => Some(missing.id.clone()),
        }
    }

    /// The system default output last moved to, while the streams follow
    /// the system default.
    pub(crate) fn seen_default(&mut self) -> Option<&mut Option<String>> {
        match self {
            Self::SystemDefault { seen_default }
            | Self::Missing(MissingOutput { seen_default, .. }) => Some(seen_default),
            Self::Device => None,
        }
    }
}
