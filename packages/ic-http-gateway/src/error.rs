//! The error module contains types for common errors that may be thrown
//! by other modules in this crate.

/// HTTP gateway result type.
pub type HttpGatewayResult<T = ()> = Result<T, HttpGatewayError>;

/// HTTP gateway error type.
#[derive(thiserror::Error, Debug)]
pub enum HttpGatewayError {
    /// Inner error from agent.
    #[error(r#"Agent error: "{0}""#)]
    AgentError(#[from] ic_agent::AgentError),

    /// Inner error from agent.
    #[error(r#"HTTP error: "{0}""#)]
    HttpError(#[from] http::Error),

    /// Inner error from agent.
    #[error(r#"HTTP header error: "{0}""#)]
    InvalidStatusCodeError(#[from] http::status::InvalidStatusCode),

    #[error(r#"Failed to parse the "{header_name}" header value: "{header_value:?}""#)]
    HeaderValueParsingError {
        header_name: String,
        header_value: String,
    },
}
