//! The error module contains types for common errors that may be thrown
//! by other modules in this crate.

use std::sync::Arc;

/// HTTP gateway result type.
pub type HttpGatewayResult<T = ()> = Result<T, HttpGatewayError>;

/// HTTP gateway error type.
#[derive(thiserror::Error, Debug, Clone)]
pub enum HttpGatewayError {
    #[error(transparent)]
    ResponseVerificationError(#[from] ic_response_verification::ResponseVerificationError),

    /// Inner error from agent.
    #[error(transparent)]
    AgentError(#[from] Arc<ic_agent::AgentError>),

    /// HTTP error.
    #[error(r#"HTTP error: "{0}""#)]
    HttpError(String),

    #[error(r#"Failed to parse the "{header_name}" header value: "{header_value:?}""#)]
    HeaderValueParsingError {
        header_name: String,
        header_value: String,
    },
}

impl From<ic_agent::AgentError> for HttpGatewayError {
    fn from(err: ic_agent::AgentError) -> Self {
        HttpGatewayError::AgentError(Arc::new(err))
    }
}

impl From<http::Error> for HttpGatewayError {
    fn from(err: http::Error) -> Self {
        HttpGatewayError::HttpError(err.to_string())
    }
}

impl From<http::status::InvalidStatusCode> for HttpGatewayError {
    fn from(err: http::status::InvalidStatusCode) -> Self {
        HttpGatewayError::HttpError(err.to_string())
    }
}
