use serde::{Serialize, Serializer};

/// Well-known error codes returned by the vnstat-rs-api.
///
/// Each variant maps to a unique integer code and a human-readable message.
/// The enum also implements [`Serialize`], serialising as the underlying
/// integer code so clients receive a compact, machine-parseable error
/// identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    /// The operation completed without error.
    NoError,
    /// Retrieving traffic data from vnstat failed.
    GetDataFailed,
    /// The requested network interface does not exist.
    NoSuchInterface,

    /// An unexpected or otherwise unclassified error occurred.
    #[allow(dead_code)]
    UnknownError,
}

impl ErrorCode {
    /// Returns the numeric error code for this variant.
    ///
    /// # Returns
    ///
    /// * `0`     — [`NoError`](ErrorCode::NoError)
    /// * `10000` — [`GetDataFailed`](ErrorCode::GetDataFailed)
    /// * `10001` — [`NoSuchInterface`](ErrorCode::NoSuchInterface)
    /// * `99999` — [`UnknownError`](ErrorCode::UnknownError)
    pub fn code(&self) -> i32 {
        match self {
            ErrorCode::NoError => 0,
            ErrorCode::GetDataFailed => 10000,
            ErrorCode::NoSuchInterface => 10001,
            ErrorCode::UnknownError => 99999,
        }
    }

    /// Returns a static human-readable description for this error variant.
    ///
    /// # Returns
    ///
    /// A `&'static str` suitable for logging or display in API responses.
    pub fn message(&self) -> &'static str {
        match self {
            ErrorCode::NoError => "No error",
            ErrorCode::GetDataFailed => "Get data failed",
            ErrorCode::UnknownError => "Unknown error",
            ErrorCode::NoSuchInterface => "No such interface",
        }
    }
}

impl Serialize for ErrorCode {
    /// Serialises this error code as its underlying `i32` value.
    ///
    /// This allows clients to receive a compact numeric code rather than a
    /// string representation of the enum variant.
    ///
    /// # Errors
    ///
    /// Propagates any error that occurs during serialisation (e.g.
    /// user-defined serialiser errors).
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i32(self.code())
    }
}
