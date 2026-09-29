use std::{error::Error as StdError, fmt};

/// Stable, coarse category for failures returned by Voxrig's public API.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ErrorKind {
    /// This version adapter does not implement the requested capability.
    Unsupported,
    /// A caller supplied an invalid value or requested an invalid operation.
    InvalidInput,
    /// Establishing or using the network connection failed.
    Connection,
    /// An operation did not complete before its configured deadline.
    Timeout,
    /// The peer closed the connection or sent a disconnect packet.
    Disconnected,
    /// A server packet was malformed, unsupported, or inconsistent.
    Protocol,
    /// A configured memory, queue, packet, or cache bound was exceeded.
    ResourceLimit,
    /// The server rejected an acknowledged operation.
    Rejected,
    /// Required game state has not arrived or is no longer available.
    State,
    /// A failure that does not fit a stable category above.
    Other,
}

/// Error returned by Voxrig's public fallible operations.
///
/// Match on [`Error::kind`] for control flow. Display text and the source chain
/// are diagnostic details and are not a stable API contract.
#[derive(Debug)]
pub struct Error {
    kind: ErrorKind,
    source: anyhow::Error,
}

impl Error {
    /// Creates an error with an explicit stable category.
    pub fn new(kind: ErrorKind, source: impl Into<anyhow::Error>) -> Self {
        Self {
            kind,
            source: source.into(),
        }
    }

    /// Returns the stable category intended for programmatic handling.
    pub const fn kind(&self) -> ErrorKind {
        self.kind
    }

    /// Returns the complete internal diagnostic chain.
    pub fn diagnostic(&self) -> &anyhow::Error {
        &self.source
    }

    fn classify(source: &anyhow::Error) -> ErrorKind {
        let message = format!("{source:#}").to_ascii_lowercase();
        if message.contains("timed out") || message.contains("timeout") {
            ErrorKind::Timeout
        } else if message.contains("disconnected") || message.contains("connection closed") {
            ErrorKind::Disconnected
        } else if message.contains("limit")
            || message.contains("too large")
            || message.contains("exceeds")
        {
            ErrorKind::ResourceLimit
        } else if message.contains("rejected") || message.contains("not accepted") {
            ErrorKind::Rejected
        } else if message.contains("connect failed")
            || message.contains("connection")
            || message.contains("broken pipe")
        {
            ErrorKind::Connection
        } else if message.contains("packet")
            || message.contains("protocol")
            || message.contains("decode")
            || message.contains("palette")
            || message.contains("nbt")
        {
            ErrorKind::Protocol
        } else if message.contains("invalid")
            || message.contains("must be")
            || message.contains("outside the supported")
        {
            ErrorKind::InvalidInput
        } else if message.contains("not open")
            || message.contains("unavailable")
            || message.contains("not ready")
        {
            ErrorKind::State
        } else {
            ErrorKind::Other
        }
    }
}

impl From<anyhow::Error> for Error {
    fn from(source: anyhow::Error) -> Self {
        Self {
            kind: Self::classify(&source),
            source,
        }
    }
}

impl From<std::io::Error> for Error {
    fn from(source: std::io::Error) -> Self {
        Self::new(ErrorKind::Connection, source)
    }
}

impl From<std::num::TryFromIntError> for Error {
    fn from(source: std::num::TryFromIntError) -> Self {
        Self::new(ErrorKind::InvalidInput, source)
    }
}

impl From<tokio::sync::broadcast::error::RecvError> for Error {
    fn from(source: tokio::sync::broadcast::error::RecvError) -> Self {
        Self::new(ErrorKind::State, source)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.source.root_cause())
    }
}

/// Result type used by Voxrig's public fallible API.
pub type Result<T> = std::result::Result<T, Error>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classification_is_stable_for_common_boundaries() {
        assert_eq!(
            Error::from(anyhow::anyhow!("play packet timed out")).kind(),
            ErrorKind::Timeout
        );
        assert_eq!(
            Error::from(anyhow::anyhow!("chunk cache limit exceeded")).kind(),
            ErrorKind::ResourceLimit
        );
        assert_eq!(
            Error::from(anyhow::anyhow!("transaction rejected")).kind(),
            ErrorKind::Rejected
        );
    }
}
