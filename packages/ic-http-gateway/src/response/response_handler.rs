use crate::consts::CONTENT_RANGE_HEADER_NAME;
use crate::{HttpGatewayResponseBody, ResponseBodyStream};
use bytes::Bytes;
use candid::Principal;
use futures::{stream, Stream, StreamExt, TryStreamExt};
use http_body::Frame;
use http_body_util::{BodyExt, Full};
use ic_agent::{Agent, AgentError};
use ic_http_certification::HttpRequest;
use ic_response_verification::MAX_VERIFICATION_VERSION;
use ic_utils::interfaces::http_request::HeaderField;
use ic_utils::{
    call::SyncCall,
    interfaces::http_request::{
        HttpRequestCanister, HttpRequestStreamingCallbackAny, HttpResponse as AgentResponse,
        StreamingCallbackHttpResponse, StreamingStrategy, Token,
    },
};
use regex::Regex;

// Limit the total number of calls to an HTTP Request loop to 1000 for now.
static MAX_HTTP_REQUEST_STREAM_CALLBACK_CALL_COUNT: usize = 1000;

// Limit the total number of calls to an HTTP request look that can be verified
static MAX_VERIFIED_HTTP_REQUEST_STREAM_CALLBACK_CALL_COUNT: usize = 4;

// Limit the number of Stream Callbacks buffered
static STREAM_CALLBACK_BUFFER: usize = 2;

pub type AgentResponseAny = AgentResponse<Token, HttpRequestStreamingCallbackAny>;

pub async fn get_body_and_streaming_body(
    agent: &Agent,
    response: &AgentResponseAny,
) -> Result<HttpGatewayResponseBody, AgentError> {
    // if we already have the full body, we can return it early
    let Some(StreamingStrategy::Callback(callback_strategy)) = response.streaming_strategy.clone()
    else {
        return Ok(HttpGatewayResponseBody::Right(Full::from(
            response.body.clone(),
        )));
    };

    let (streamed_body, token) = create_stream(
        agent.clone(),
        callback_strategy.callback.clone(),
        Some(callback_strategy.token),
    )
    .take(MAX_VERIFIED_HTTP_REQUEST_STREAM_CALLBACK_CALL_COUNT)
    .map(|x| async move { x })
    .buffered(STREAM_CALLBACK_BUFFER)
    .try_fold(
        (vec![], None::<Token>),
        |mut accum, (mut body, token)| async move {
            accum.0.append(&mut body);
            accum.1 = token;

            Ok(accum)
        },
    )
    .await?;

    let streamed_body = [response.body.clone(), streamed_body].concat();

    // if we still have a token at this point,
    // we were unable to collect the response within the allowed certified callback limit,
    // fallback to uncertified streaming using what we've streamed so far as the initial body
    if token.is_some() {
        let body_stream = create_body_stream(
            agent.clone(),
            callback_strategy.callback,
            token,
            streamed_body,
        );

        return Ok(HttpGatewayResponseBody::Left(body_stream));
    };

    // if we no longer have a token at this point,
    // we were able to collect the response within the allow certified callback limit,
    // return this collected response as a standard response body so it will be verified
    Ok(HttpGatewayResponseBody::Right(Full::from(streamed_body)))
}

fn create_body_stream(
    agent: Agent,
    callback: HttpRequestStreamingCallbackAny,
    token: Option<Token>,
    initial_body: Vec<u8>,
) -> ResponseBodyStream {
    let chunks_stream = create_stream(agent, callback, token)
        .map(|chunk| chunk.map(|(body, _)| Frame::data(Bytes::from(body))));

    let body_stream = stream::once(async move { Ok(Frame::data(Bytes::from(initial_body))) })
        .chain(chunks_stream)
        .take(MAX_HTTP_REQUEST_STREAM_CALLBACK_CALL_COUNT)
        .map(|x| async move { x })
        .buffered(STREAM_CALLBACK_BUFFER);

    ResponseBodyStream::new(Box::pin(body_stream))
}

fn create_stream(
    agent: Agent,
    callback: HttpRequestStreamingCallbackAny,
    token: Option<Token>,
) -> impl Stream<Item = Result<(Vec<u8>, Option<Token>), AgentError>> {
    futures::stream::try_unfold(
        (agent, callback, token),
        |(agent, callback, token)| async move {
            let Some(token) = token else {
                return Ok(None);
            };

            let canister = HttpRequestCanister::create(&agent, callback.0.principal);
            match canister
                .http_request_stream_callback(&callback.0.method, token)
                .call()
                .await
            {
                Ok((StreamingCallbackHttpResponse { body, token },)) => {
                    Ok(Some(((body, token.clone()), (agent, callback, token))))
                }
                Err(e) => Err(e),
            }
        },
    )
}

#[derive(Clone, Debug)]
struct StreamState {
    pub http_request: HttpRequest,
    pub canister_id: Principal,
    pub total_length: usize,
    pub fetched_length: usize,
}

pub async fn get_206_stream_response_body(
    agent: &Agent,
    http_request: &HttpRequest,
    canister_id: Principal,
    response_headers: &Vec<HeaderField<'static>>,
    response_206_body: HttpGatewayResponseBody,
) -> Result<HttpGatewayResponseBody, AgentError> {
    let HttpGatewayResponseBody::Right(body) = response_206_body else {
        return Err(AgentError::InvalidHttpResponse(
            "Expected full 206 response".to_string(),
        ));
    };
    // The expect below should never panic because `Either::Right` will always have a full body
    let streamed_body = body
        .collect()
        .await
        .expect("missing streamed chunk body")
        .to_bytes()
        .to_vec();
    let stream_state = get_stream_state(http_request, canister_id, response_headers)?;

    let body_stream = create_206_body_stream(agent.clone(), stream_state, streamed_body);
    return Ok(HttpGatewayResponseBody::Left(body_stream));
}

#[derive(Debug)]
struct ContentRangeValues {
    pub range_begin: usize,
    pub range_end: usize,
    pub total_length: usize,
}

fn parse_content_range_header_str(str_value: &str) -> Result<ContentRangeValues, AgentError> {
    // expected format: `bytes 21010-47021/47022`
    let re = Regex::new(r"bytes\s+(\d+)-(\d+)/(\d+)").unwrap();
    let Some(caps) = re.captures(str_value) else {
        return Err(AgentError::InvalidHttpResponse(
            "malformed Content-Range header".to_string(),
        ));
    };
    let range_begin: usize = caps
        .get(1)
        .expect("missing range-begin")
        .as_str()
        .parse()
        .map_err(|_| AgentError::InvalidHttpResponse("malformed range-begin".to_string()))?;
    let range_end: usize = caps
        .get(2)
        .expect("missing range-end")
        .as_str()
        .parse()
        .map_err(|_| AgentError::InvalidHttpResponse("malformed range-end".to_string()))?;
    let total_length: usize = caps
        .get(3)
        .expect("missing size")
        .as_str()
        .parse()
        .map_err(|_| AgentError::InvalidHttpResponse("malformed size".to_string()))?;
    // TODO: add sanity checks for the parsed values
    return Ok(ContentRangeValues {
        range_begin,
        range_end,
        total_length,
    });
}

fn get_content_range_header_str(
    response_headers: &Vec<HeaderField<'static>>,
) -> Result<String, AgentError> {
    for HeaderField(name, value) in response_headers {
        if name.eq_ignore_ascii_case(CONTENT_RANGE_HEADER_NAME) {
            return Ok(value.to_string());
        }
    }
    Err(AgentError::InvalidHttpResponse(
        "missing Content-Range header in 206 response".to_string(),
    ))
}

fn get_content_range_values(
    response_headers: &Vec<HeaderField<'static>>,
) -> Result<ContentRangeValues, AgentError> {
    let str_value = get_content_range_header_str(response_headers)?;
    parse_content_range_header_str(&str_value)
}

fn get_stream_state(
    http_request: &HttpRequest,
    canister_id: Principal,
    response_headers: &Vec<HeaderField<'static>>,
) -> Result<StreamState, AgentError> {
    let range_values = get_content_range_values(response_headers)?;

    Ok(StreamState {
        http_request: http_request.clone(),
        canister_id,
        total_length: range_values.total_length,
        fetched_length: range_values
            .range_end
            .saturating_sub(range_values.range_begin)
            + 1,
    })
}

fn create_206_body_stream(
    agent: Agent,
    stream_state: StreamState,
    initial_body: Vec<u8>,
) -> ResponseBodyStream {
    let chunks_stream = create_206_stream(agent, Some(stream_state))
        .map(|chunk| chunk.map(|(body, _)| Frame::data(Bytes::from(body))));

    let body_stream = stream::once(async move { Ok(Frame::data(Bytes::from(initial_body))) })
        .chain(chunks_stream)
        .take(MAX_HTTP_REQUEST_STREAM_CALLBACK_CALL_COUNT)
        .map(|x| async move { x })
        .buffered(STREAM_CALLBACK_BUFFER);

    ResponseBodyStream::new(Box::pin(body_stream))
}

fn create_206_stream(
    agent: Agent,
    maybe_stream_state: Option<StreamState>,
) -> impl Stream<Item = Result<(Vec<u8>, Option<StreamState>), AgentError>> {
    futures::stream::try_unfold(
        (agent, maybe_stream_state),
        |(agent, maybe_stream_state)| async move {
            let Some(stream_state) = maybe_stream_state else {
                return Ok(None);
            };
            let canister = HttpRequestCanister::create(&agent, stream_state.canister_id);
            let next_chunk_begin = stream_state.fetched_length;
            let mut updated_headers = stream_state.http_request.headers.clone();
            updated_headers.push(("Range".to_string(), format!("bytes={}-", next_chunk_begin)));
            let headers = updated_headers
                .iter()
                .map(|(name, value)| HeaderField(name.into(), value.into()))
                .collect::<Vec<HeaderField>>()
                .into_iter();
            let query_result = canister
                .http_request(
                    &stream_state.http_request.method,
                    &stream_state.http_request.url,
                    headers,
                    &stream_state.http_request.body,
                    Some(&u16::from(MAX_VERIFICATION_VERSION)),
                )
                .call()
                .await;
            let agent_response = match query_result {
                Ok((response,)) => response,
                Err(e) => return Err(e),
            };
            let range_values = get_content_range_values(&agent_response.headers)?;
            let chunk_length = range_values
                .range_end
                .saturating_sub(range_values.range_begin)
                + 1;
            let current_fetched_length = stream_state.fetched_length + chunk_length;
            // TODO: verify the range response, once we can prepare certified chunks
            let maybe_new_state = if current_fetched_length < stream_state.total_length {
                Some(StreamState {
                    fetched_length: current_fetched_length,
                    ..stream_state
                })
            } else {
                None
            };
            Ok(Some((
                (agent_response.body, maybe_new_state.clone()),
                (agent, maybe_new_state),
            )))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use assert_matches::assert_matches;
    use std::borrow::Cow;

    #[test]
    fn should_parse_content_range_header_str() {
        let header_values = [
            ContentRangeValues {
                range_begin: 0,
                range_end: 0,
                total_length: 1,
            },
            ContentRangeValues {
                range_begin: 100,
                range_end: 2000,
                total_length: 3000,
            },
            ContentRangeValues {
                range_begin: 10_000,
                range_end: 300_000,
                total_length: 500_000,
            },
        ];
        for v in header_values {
            let input = format!("bytes {}-{}/{}", v.range_begin, v.range_end, v.total_length);
            let result = parse_content_range_header_str(&input);
            let output = result.expect(&format!("failed parsing '{}'", input));
            assert_eq!(v.range_begin, output.range_begin);
            assert_eq!(v.range_end, output.range_end);
            assert_eq!(v.total_length, output.total_length);
        }
    }

    #[test]
    fn should_fail_parse_content_range_header_str_on_malformed_input() {
        let malformed_inputs = [
            "byte 1-2/3",
            "bites 2-4/8",
            "bytes 100-200/asdf",
            "bytes 12345",
            "something else",
            "bytes dead-beef/123456",
        ];
        for input in malformed_inputs {
            let result = parse_content_range_header_str(&input);
            assert_matches!(result, Err(e) if format!("{}", e).contains("malformed Content-Range header"));
        }
    }

    #[test]
    fn should_get_stream_state() {
        let http_request = HttpRequest {
            method: "GET".to_string(),
            url: "http://example.com/some_file".to_string(),
            headers: vec![("Xyz".to_string(), "some value".to_string())],
            body: vec![42],
        };
        let canister_id = Principal::from_slice(&[1, 2, 3, 4]);
        let response_headers = vec![HeaderField(
            Cow::from("Content-Range"),
            Cow::from("bytes 2-4/10"), // fetched 3 bytes, total length is 10
        )];
        let state = get_stream_state(&http_request, canister_id, &response_headers)
            .expect("failed constructing StreamState");
        assert_eq!(state.http_request, http_request);
        assert_eq!(state.canister_id, canister_id);
        assert_eq!(state.fetched_length, 3);
        assert_eq!(state.total_length, 10);
    }

    #[test]
    fn should_fail_get_stream_state_without_content_range_header() {
        let http_request = HttpRequest {
            method: "GET".to_string(),
            url: "http://example.com/some_file".to_string(),
            headers: vec![("Xyz".to_string(), "some value".to_string())],
            body: vec![42],
        };
        let canister_id = Principal::from_slice(&[1, 2, 3, 4]);
        let response_headers = vec![HeaderField(
            Cow::from("other header"),
            Cow::from("other value"),
        )];
        let result = get_stream_state(&http_request, canister_id, &response_headers);
        assert_matches!(result, Err(e) if format!("{}", e).contains("missing Content-Range header"));
    }

    #[test]
    fn should_fail_get_stream_state_with_malformed_content_range_header() {
        let http_request = HttpRequest {
            method: "GET".to_string(),
            url: "http://example.com/some_file".to_string(),
            headers: vec![("Xyz".to_string(), "some value".to_string())],
            body: vec![42],
        };
        let canister_id = Principal::from_slice(&[1, 2, 3, 4]);
        let response_headers = vec![HeaderField(
            Cow::from("Content-Range"),
            Cow::from("bytes 42/10"),
        )];
        let result = get_stream_state(&http_request, canister_id, &response_headers);
        assert_matches!(result, Err(e) if format!("{}", e).contains("malformed Content-Range header"));
    }
}
