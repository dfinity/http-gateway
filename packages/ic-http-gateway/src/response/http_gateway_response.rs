use bytes::Bytes;
use futures::Stream;
use http::Response;
use http_body::{Body, Frame, SizeHint};
use ic_agent::AgentError;
use std::{
    fmt::{Debug, Formatter},
    pin::Pin,
    task::{Context, Poll},
};

use crate::HttpGatewayError;

pub type CanisterResponse = Response<HttpGatewayResponseBody>;

/// A response from the HTTP gateway.
#[derive(Debug)]
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

/// The body of an HTTP gateway response.
#[derive(Debug)]
pub enum HttpGatewayResponseBody {
    /// A byte array representing the response body.
    Bytes(Vec<u8>),

    /// A stream of response body chunks.
    Stream(ResponseBodyStream),
}

impl Body for HttpGatewayResponseBody {
    type Data = Bytes;
    type Error = AgentError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.get_mut() {
            HttpGatewayResponseBody::Bytes(bytes) => {
                Poll::Ready(Some(Ok(Frame::data(Bytes::from(bytes.clone())))))
            }
            HttpGatewayResponseBody::Stream(stream) => Stream::poll_next(Pin::new(stream), cx),
        }
    }

    fn is_end_stream(&self) -> bool {
        match self {
            HttpGatewayResponseBody::Bytes(_) => true,
            HttpGatewayResponseBody::Stream(_) => false,
        }
    }

    fn size_hint(&self) -> SizeHint {
        match self {
            HttpGatewayResponseBody::Bytes(bytes) => SizeHint::with_exact(bytes.len() as u64),
            HttpGatewayResponseBody::Stream(stream) => {
                let (lower, upper) = stream.size_hint();

                let mut size_hint = SizeHint::new();
                size_hint.set_lower(lower as u64);

                if let Some(upper) = upper {
                    size_hint.set_upper(upper as u64);
                }

                size_hint
            }
        }
    }
}

/// An item in a response body stream.
pub type ResponseBodyStreamItem = Result<Frame<Bytes>, AgentError>;

/// A stream of response body chunks.
pub struct ResponseBodyStream {
    inner: Pin<Box<dyn Stream<Item = ResponseBodyStreamItem> + 'static>>,
}

impl ResponseBodyStream {
    pub fn new(stream: impl Stream<Item = ResponseBodyStreamItem> + 'static) -> Self {
        Self {
            inner: Box::pin(stream),
        }
    }
}

impl Debug for ResponseBodyStream {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResponseBodyStream").finish()
    }
}

impl Stream for ResponseBodyStream {
    type Item = ResponseBodyStreamItem;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.inner.as_mut().poll_next(cx)
    }
}
