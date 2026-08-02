use crate::error_code::ErrorCode;
use serde::Serialize;

/// A standardized JSON response following the [JSend specification](https://github.com/omniti-labs/jsend).
///
/// This struct is used as the uniform envelope for all API responses. It carries a
/// `status` string, an application-specific `code`, an optional human-readable
/// `message`, and an optional `data` payload.
///
/// # Type parameters
///
/// * `T` — The type of the optional data payload, which must implement [`Serialize`].
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsendResponse<T>
where
    T: Serialize,
{
    /// The status of the response. Either `"success"` or `"fail"`.
    pub status: String,

    /// An application-specific error code. [`ErrorCode::NoError`] indicates success.
    pub code: ErrorCode,

    /// An optional human-readable message. Present only when the response represents
    /// a failure condition.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,

    /// An optional data payload. Present only when the response is successful and
    /// carries meaningful data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T> JsendResponse<T>
where
    T: Serialize,
{
    /// Creates a success response with no data payload.
    ///
    /// The returned response has a `"success"` status, [`ErrorCode::NoError`], and
    /// both `message` and `data` set to `None`.
    #[allow(dead_code)]
    pub fn success() -> Self {
        Self {
            status: "success".to_string(),
            code: ErrorCode::NoError,
            message: None,
            data: None,
        }
    }

    /// Creates a success response containing the given data.
    ///
    /// # Arguments
    ///
    /// * `data` — The payload to include in the response.
    ///
    /// The returned response has a `"success"` status, [`ErrorCode::NoError`], and
    /// `message` set to `None`.
    pub fn success_with_data(data: T) -> Self {
        Self {
            status: "success".to_string(),
            code: ErrorCode::NoError,
            message: None,
            data: Some(data),
        }
    }

    /// Creates a fail response with the given error code.
    ///
    /// The human-readable `message` is automatically populated from the error code's
    /// default message. The `data` field is set to `None`.
    ///
    /// # Arguments
    ///
    /// * `code` — The application-specific error code describing the failure.
    #[allow(dead_code)]
    pub fn fail(code: ErrorCode) -> Self {
        Self {
            status: "fail".to_string(),
            message: Some(code.message().to_string()),
            code,
            data: None,
        }
    }

    /// Creates a fail response with a custom message.
    ///
    /// The returned response has a `"fail"` status (client-side failure).
    pub fn fail_with_message(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: "fail".to_string(),
            code,
            message: Some(message.into()),
            data: None,
        }
    }

    /// Creates an error response with a custom message.
    ///
    /// The returned response has an `"error"` status (server-side failure).
    pub fn error(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status: "error".to_string(),
            code,
            message: Some(message.into()),
            data: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn success_response_omits_message_and_data() {
        let json = serde_json::to_string(&JsendResponse::<()>::success()).unwrap();
        assert_eq!(json, r#"{"status":"success","code":0}"#);
    }

    #[test]
    fn success_with_data_includes_payload() {
        let json = serde_json::to_string(&JsendResponse::success_with_data(42)).unwrap();
        assert_eq!(json, r#"{"status":"success","code":0,"data":42}"#);
    }

    #[test]
    fn fail_response_includes_default_message() {
        let json =
            serde_json::to_string(&JsendResponse::<()>::fail(ErrorCode::NoSuchInterface)).unwrap();
        assert_eq!(
            json,
            r#"{"status":"fail","code":10001,"message":"No such interface"}"#
        );
    }

    #[test]
    fn fail_with_message_uses_custom_message() {
        let json = serde_json::to_string(&JsendResponse::<()>::fail_with_message(
            ErrorCode::NoSuchInterface,
            "no such interface: eth9",
        ))
        .unwrap();
        assert_eq!(
            json,
            r#"{"status":"fail","code":10001,"message":"no such interface: eth9"}"#
        );
    }

    #[test]
    fn error_response_uses_error_status() {
        let json = serde_json::to_string(&JsendResponse::<()>::error(
            ErrorCode::GetDataFailed,
            "boom",
        ))
        .unwrap();
        assert_eq!(json, r#"{"status":"error","code":10000,"message":"boom"}"#);
    }
}
