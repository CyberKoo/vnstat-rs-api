use serde::{Serialize, Serializer};

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCode {
    NoError,
    GetDataFailed,
    NoSuchInterface,

    UnknownError,
}

impl ErrorCode {
    pub fn code(&self) -> i32 {
        match self {
            ErrorCode::NoError => 0,
            ErrorCode::GetDataFailed => 10000,
            ErrorCode::NoSuchInterface => 10001,
            ErrorCode::UnknownError => 99999,
        }
    }
    pub fn message(&self) -> &'static str {
        match self {
            ErrorCode::NoError => "No error",
            ErrorCode::GetDataFailed => "Get data failed",
            ErrorCode::UnknownError => "Unknown error",
            &ErrorCode::NoSuchInterface => "No such interface",
        }
    }
}

// 让 ErrorCode 序列化为整数
impl Serialize for ErrorCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i32(self.code())
    }
}
