use std::io;

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error: {0}")]
    Io(#[from] io::Error),

    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),

    #[error("TLS error: {0}")]
    Tls(#[from] rustls::Error),

    #[error("certificate generation failed: {0}")]
    Cert(#[from] rcgen::Error),

    #[error("could not connect: {0}")]
    Connect(#[from] quinn::ConnectError),

    #[error("connection lost: {0}")]
    Connection(#[from] quinn::ConnectionError),

    #[error("peer sent malformed data: {0}")]
    Protocol(String),

    #[error("device is offline or unreachable")]
    Unreachable,

    #[error("device not found on the local network")]
    NotFound,

    #[error("device is not paired with us")]
    NotTrusted,

    #[error("peer identity does not match the paired key")]
    IdentityMismatch,

    #[error("wrong password")]
    WrongPassword,

    #[error("too many failed attempts, try again in {0} seconds")]
    LockedOut(u64),

    #[error("rejected by the receiver")]
    Rejected,

    #[error("transfer was cancelled")]
    Cancelled,

    #[error("transfer was paused")]
    Paused,

    #[error("invalid path: {0}")]
    InvalidPath(String),

    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    #[error("unknown transfer")]
    UnknownTransfer,

    #[error("data corrupted in transit: {0}")]
    Integrity(String),

    #[error("timed out")]
    Timeout,
}

impl Error {
    pub(crate) fn protocol(msg: impl Into<String>) -> Self {
        Error::Protocol(msg.into())
    }
}

impl From<serde_json::Error> for Error {
    fn from(e: serde_json::Error) -> Self {
        Error::Protocol(format!("json: {e}"))
    }
}

impl From<quinn::WriteError> for Error {
    fn from(e: quinn::WriteError) -> Self {
        match e {
            quinn::WriteError::ConnectionLost(c) => Error::Connection(c),
            other => Error::Protocol(format!("stream write: {other}")),
        }
    }
}

impl From<quinn::ReadError> for Error {
    fn from(e: quinn::ReadError) -> Self {
        match e {
            quinn::ReadError::ConnectionLost(c) => Error::Connection(c),
            other => Error::Protocol(format!("stream read: {other}")),
        }
    }
}

impl From<quinn::ReadExactError> for Error {
    fn from(e: quinn::ReadExactError) -> Self {
        match e {
            quinn::ReadExactError::ReadError(r) => r.into(),
            quinn::ReadExactError::FinishedEarly(_) => Error::protocol("stream ended early"),
        }
    }
}

impl From<quinn::ClosedStream> for Error {
    fn from(_: quinn::ClosedStream) -> Self {
        Error::protocol("stream already closed")
    }
}

impl From<tokio::time::error::Elapsed> for Error {
    fn from(_: tokio::time::error::Elapsed) -> Self {
        Error::Timeout
    }
}
