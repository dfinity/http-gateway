use bytes::Bytes;
use futures::stream::BoxStream;
use http::Response;
use http_body::Frame;
use http_body_util::{Either, Full, StreamBody};
use ic_agent::AgentError;
use std::fmt::Debug;

use crate::HttpGatewayError;

pub type CanisterResponse = Response<HttpGatewayResponseBody>;

/// A response from the HTTP gateway.
pub struct HttpGatewayResponse {
    /// The certified response, excluding uncertified headers.
    /// If response verification v1 is used, the original, uncertified headers are returned.
    pub canister_response: CanisterResponse,

    /// Additional metadata regarding the response.
    pub metadata: HttpGatewayResponseMetadata,
}

/// Additional metadata regarding the response.
#[derive(Debug, Clone)]
pub struct HttpGatewayResponseMetadata {
    /// Whether the original query call was upgraded to an update call.
    pub upgraded_to_update_call: bool,

    /// The version of response verification that was used to verify the response.
    /// If the protocol fails before getting to the verification step, or the
    /// original query call is upgraded to an update call, this field will be `None`.
    pub response_verification_version: Option<u16>,

    /// The internal error that resulted in the HTTP response being an error response.
    pub internal_error: Option<HttpGatewayError>,
}

pub type HttpGatewayResponseBody = Either<ResponseBodyStream, Full<Bytes>>;

pub type ResponseBodyStream = StreamBody<BoxStream<'static, ResponseBodyStreamItem>>;

/// An item in a response body stream.
pub type ResponseBodyStreamItem = Result<Frame<Bytes>, AgentError>;
