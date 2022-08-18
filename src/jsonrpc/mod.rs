mod api;
pub mod server;

use std::{error, fmt};

use serde::{self, Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum Params {
    Array(Vec<serde_json::Value>),
    Map(serde_json::Map<String, serde_json::Value>),
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
pub enum ReqId {
    Num(u64),
    Str(String),
}

/// A JSONRPC2 request. See https://www.jsonrpc.org/specification#request_object.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    /// Version. Must be "2.0".
    pub jsonrpc: String,
    /// Command name.
    pub method: String,
    /// Command parameters.
    pub params: Option<Params>,
    /// Request identifier.
    pub id: ReqId,
}

/// JSONRPC2 error codes. See https://www.jsonrpc.org/specification#error_object.
#[derive(Debug, PartialEq, Clone)]
pub enum ErrorCode {
    /// The method does not exist / is not available.
    MethodNotFound,
    /// Invalid method parameter(s).
    InvalidParams,
    /// Reserved for implementation-defined server-errors.
    ServerError(i64),
}

impl Into<i64> for &ErrorCode {
    fn into(self) -> i64 {
        match self {
            ErrorCode::MethodNotFound => -32601,
            ErrorCode::InvalidParams => -32602,
            ErrorCode::ServerError(code) => *code,
        }
    }
}

impl From<i64> for ErrorCode {
    fn from(code: i64) -> ErrorCode {
        match code {
            -32601 => ErrorCode::MethodNotFound,
            -32602 => ErrorCode::InvalidParams,
            code => ErrorCode::ServerError(code),
        }
    }
}

impl<'a> Deserialize<'a> for ErrorCode {
    fn deserialize<D>(deserializer: D) -> Result<ErrorCode, D::Error>
    where
        D: Deserializer<'a>,
    {
        let code: i64 = Deserialize::deserialize(deserializer)?;
        Ok(code.into())
    }
}

impl Serialize for ErrorCode {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_i64(self.into())
    }
}

/// JSONRPC2 error response. See https://www.jsonrpc.org/specification#error_object.
#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Error {
    pub code: ErrorCode,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl Error {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Error {
        Error {
            message: message.into(),
            code,
            data: None,
        }
    }

    pub fn method_not_found() -> Error {
        Error::new(ErrorCode::MethodNotFound, "Method not found")
    }

    pub fn invalid_params<M>(message: impl Into<String>) -> Error {
        Error::new(
            ErrorCode::InvalidParams,
            format!("Invalid params: {}", message.into()),
        )
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let code: i64 = (&self.code).into();
        write!(f, "{}: {}", code, self.message)
    }
}

impl error::Error for Error {}

/// JSONRPC2 response. See https://www.jsonrpc.org/specification#response_object.
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Response {
    /// Version. Must be "2.0".
    jsonrpc: String,
    /// Required on success. Must not exist on error.
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<serde_json::Value>,
    /// Required on error. Must not exist on success.
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<Error>,
    /// Request identifier.
    id: ReqId,
}

impl Response {
    fn new(id: ReqId, result: Option<serde_json::Value>, error: Option<Error>) -> Response {
        Response {
            jsonrpc: "2.0".to_string(),
            result,
            error,
            id,
        }
    }

    pub fn success(id: ReqId, result: serde_json::Value) -> Response {
        Response::new(id, Some(result), None)
    }

    pub fn error(id: ReqId, error: Error) -> Response {
        Response::new(id, None, Some(error))
    }
}
