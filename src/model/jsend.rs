#![allow(dead_code)]
use crate::error_code::ErrorCode;
use serde::Serialize;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JsendResponse<T>
where
    T: Serialize,
{
    pub status: String,
    pub code: ErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<T>,
}

impl<T> JsendResponse<T>
where
    T: Serialize,
{
    pub fn success() -> Self {
        Self {
            status: "success".to_string(),
            code: ErrorCode::NoError,
            message: None,
            data: None,
        }
    }

    pub fn success_with_data(data: T) -> Self {
        Self {
            status: "success".to_string(),
            code: ErrorCode::NoError,
            message: None,
            data: Some(data),
        }
    }

    pub fn success_with_data_and_code(data: T, code: ErrorCode) -> Self {
        Self {
            status: "success".to_string(),
            code,
            message: None,
            data: Some(data),
        }
    }

    pub fn fail(code: ErrorCode) -> Self {
        Self {
            status: "fail".to_string(),
            message: Some(code.message().to_string()),
            code,
            data: None,
        }
    }

    pub fn fail_with_message(message: String, code: ErrorCode) -> Self {
        Self {
            status: "fail".to_string(),
            message: Some(message),
            code,
            data: None,
        }
    }

    pub fn fail_with_message_data(message: String, data: T, code: ErrorCode) -> Self {
        Self {
            status: "fail".to_string(),
            message: Some(message),
            code,
            data: Some(data),
        }
    }

    pub fn error(code: ErrorCode) -> Self {
        Self {
            status: "error".to_string(),
            message: Some(code.message().to_string()),
            code,
            data: None,
        }
    }

    pub fn error_with_message(message: String, code: ErrorCode) -> Self {
        Self {
            status: "error".to_string(),
            message: Some(message),
            code,
            data: None,
        }
    }

    pub fn error_with_message_data(message: String, data: T, code: ErrorCode) -> Self {
        Self {
            status: "error".to_string(),
            message: Some(message),
            code,
            data: Some(data),
        }
    }
}
