//! The error module contains types for common errors that may be thrown
//! by other modules in this crate.

use ic_agent::AgentError;
use ic_response_verification::ResponseVerificationError;
use std::sync::Arc;

/// HTTP gateway result type.
pub type HttpGatewayResult<T = ()> = Result<T, HttpGatewayError>;

/// HTTP gateway error type.
#[derive(thiserror::Error, Debug, Clone)]
pub enum HttpGatewayError {
    #[error(transparent)]
    ResponseVerificationError(#[from] ResponseVerificationError),

    /// Inner error from agent.
    #[error(transparent)]
    AgentError(#[from] Arc<AgentError>),

    /// HTTP error.
    #[error(transparent)]
    HttpError(#[from] Arc<http::Error>),

    #[error(r#"Failed to parse the "{header_name}" header value: "{header_value:?}""#)]
    HeaderValueParsingError {
        header_name: String,
        header_value: String,
    },
}

impl From<AgentError> for HttpGatewayError {
    fn from(err: AgentError) -> Self {
        HttpGatewayError::AgentError(Arc::new(err))
    }
}

impl From<http::Error> for HttpGatewayError {
    fn from(err: http::Error) -> Self {
        HttpGatewayError::HttpError(Arc::new(err))
    }
}
