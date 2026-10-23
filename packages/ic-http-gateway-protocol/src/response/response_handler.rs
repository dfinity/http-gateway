use crate::protocol::validate;
use crate::{HttpGatewayResponseBody, ResponseBodyStream, ResponseBodyStreamItem};
use bytes::Bytes;
use candid::Principal;
use futures::{future, stream, Stream, StreamExt, TryStreamExt};
use http_body::Frame;
use http_body_util::{BodyExt, Full};
use ic_agent::{Agent, AgentError};
use ic_http_certification::{HttpRequest, HttpResponse, StatusCode};
use ic_response_verification::MAX_VERIFICATION_VERSION;
use ic_utils::interfaces::http_request::HeaderField;
use ic_utils::{
    call::SyncCall,
    interfaces::http_request::{
        HttpRequestCanister, HttpRequestStreamingCallbackAny, HttpResponse as AgentResponse,
        StreamingCallbackHttpResponse, StreamingStrategy, Token,
    },
};

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
    let chunks_stream = create_stream(agent, callback, token).map_ok(|(body, _)| body);

    let body_stream = stream::once(async move { Ok(initial_body) })
        .chain(chunks_stream)
        .take(MAX_HTTP_REQUEST_STREAM_CALLBACK_CALL_COUNT)
        .map(|x| async move { x })
        .buffered(STREAM_CALLBACK_BUFFER);

    ResponseBodyStream::new(Box::pin(chunks_to_frames(body_stream)))
}

/// Converts a stream of body chunks into a stream of DATA frames, skipping empty chunks.
///
/// The end of the body is signalled by the end of the stream, not by a DATA frame, so an empty
/// DATA frame carries no information. Over HTTP/2 it would be sent as an empty DATA frame without
/// the END_STREAM flag, so no empty DATA frames are emitted at all.
///
/// This must be applied after `.take(MAX_HTTP_REQUEST_STREAM_CALLBACK_CALL_COUNT)`, so that
/// skipped chunks still count towards the limit on the number of calls made to the canister.
fn chunks_to_frames<S>(chunks: S) -> impl Stream<Item = ResponseBodyStreamItem>
where
    S: Stream<Item = Result<Vec<u8>, AgentError>>,
{
    chunks
        .try_filter(|body| future::ready(!body.is_empty()))
        .map_ok(|body| Frame::data(Bytes::from(body)))
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
struct StreamState<'a> {
    pub http_request: HttpRequest<'a>,
    pub canister_id: Principal,
    pub total_length: usize,
    pub fetched_length: usize,
    pub skip_verification: bool,
}

pub async fn get_206_stream_response_body_and_total_length(
    agent: &Agent,
    http_request: HttpRequest<'static>,
    canister_id: Principal,
    response_headers: &Vec<HeaderField<'static>>,
    response_206_body: HttpGatewayResponseBody,
    skip_verification: bool,
) -> Result<(HttpGatewayResponseBody, usize), AgentError> {
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
    let stream_state = get_initial_stream_state(
        http_request,
        canister_id,
        response_headers,
        streamed_body.len(),
        skip_verification,
    )?;
    let content_length = stream_state.total_length;

    let body_stream = create_206_body_stream(agent.clone(), stream_state, streamed_body);
    Ok((HttpGatewayResponseBody::Left(body_stream), content_length))
}

#[derive(Debug)]
struct ContentRangeValues {
    pub range_begin: usize,
    pub range_end: usize,
    pub total_length: usize,
}

fn parse_content_range_header_str(
    content_range_str: &str,
) -> Result<ContentRangeValues, AgentError> {
    // expected format: `bytes 21010-47021/47022`
    let str_value = content_range_str.trim();
    if !str_value.starts_with("bytes ") {
        return Err(AgentError::InvalidHttpResponse(format!(
            "Invalid Content-Range header '{}'",
            content_range_str
        )));
    }
    let str_value = str_value.trim_start_matches("bytes ");

    let str_value_parts = str_value.split('-').collect::<Vec<_>>();
    if str_value_parts.len() != 2 {
        return Err(AgentError::InvalidHttpResponse(format!(
            "Invalid bytes spec in Content-Range header '{}'",
            content_range_str
        )));
    }
    let range_begin = str_value_parts[0].parse::<usize>().map_err(|e| {
        AgentError::InvalidHttpResponse(format!(
            "Invalid range_begin in '{}': {}",
            content_range_str, e
        ))
    })?;

    let other_value_parts = str_value_parts[1].split('/').collect::<Vec<_>>();
    if other_value_parts.len() != 2 {
        return Err(AgentError::InvalidHttpResponse(format!(
            "Invalid bytes spec in Content-Range header '{}'",
            content_range_str
        )));
    }
    let range_end = other_value_parts[0].parse::<usize>().map_err(|e| {
        AgentError::InvalidHttpResponse(format!(
            "Invalid range_end in '{}': {}",
            content_range_str, e
        ))
    })?;
    let total_length = other_value_parts[1].parse::<usize>().map_err(|e| {
        AgentError::InvalidHttpResponse(format!(
            "Invalid total_length in '{}': {}",
            content_range_str, e
        ))
    })?;

    let rv = ContentRangeValues {
        range_begin,
        range_end,
        total_length,
    };
    if rv.range_begin > rv.range_end
        || rv.range_begin >= rv.total_length
        || rv.range_end >= rv.total_length
    {
        Err(AgentError::InvalidHttpResponse(format!(
            "inconsistent Content-Range header {}: {:?}",
            content_range_str, rv
        )))
    } else {
        Ok(rv)
    }
}

fn get_content_range_header_str(
    response_headers: &Vec<HeaderField<'static>>,
) -> Result<String, AgentError> {
    for HeaderField(name, value) in response_headers {
        if name.eq_ignore_ascii_case(http::header::CONTENT_RANGE.as_ref()) {
            return Ok(value.to_string());
        }
    }
    Err(AgentError::InvalidHttpResponse(
        "missing Content-Range header in 206 response".to_string(),
    ))
}

fn get_content_range_values(
    response_headers: &Vec<HeaderField<'static>>,
    fetched_length: usize,
    body_length: usize,
) -> Result<ContentRangeValues, AgentError> {
    let str_value = get_content_range_header_str(response_headers)?;
    let range_values = parse_content_range_header_str(&str_value)?;

    if range_values.range_begin > fetched_length {
        return Err(AgentError::InvalidHttpResponse(format!(
            "chunk out-of-order: range_begin={} is larger than expected begin={} ",
            range_values.range_begin, fetched_length
        )));
    }
    if range_values.range_end < fetched_length {
        return Err(AgentError::InvalidHttpResponse(format!(
            "chunk out-of-order: range_end={} is smaller than length fetched so far={} ",
            range_values.range_end, fetched_length
        )));
    }
    // The Content-Range header is controlled by the canister, so it must match the body,
    // otherwise the offsets computed from it cannot be used to slice the body.
    // `parse_content_range_header_str` guarantees that `range_begin <= range_end`.
    let range_length = range_values.range_end - range_values.range_begin + 1;
    if body_length != range_length {
        return Err(AgentError::InvalidHttpResponse(format!(
            "Content-Range header {} does not match body length={}",
            str_value, body_length
        )));
    }
    Ok(range_values)
}

fn get_initial_stream_state<'a>(
    http_request: HttpRequest<'a>,
    canister_id: Principal,
    response_headers: &Vec<HeaderField<'static>>,
    initial_body_length: usize,
    skip_verification: bool,
) -> Result<StreamState<'a>, AgentError> {
    let range_values = get_content_range_values(response_headers, 0, initial_body_length)?;

    Ok(StreamState {
        http_request,
        canister_id,
        total_length: range_values.total_length,
        fetched_length: range_values
            .range_end
            .saturating_sub(range_values.range_begin)
            + 1,
        skip_verification,
    })
}

fn create_206_body_stream(
    agent: Agent,
    stream_state: StreamState<'static>,
    initial_body: Vec<u8>,
) -> ResponseBodyStream {
    let chunks_stream = create_206_stream(agent, Some(stream_state)).map_ok(|(body, _)| body);

    let body_stream = stream::once(async move { Ok(initial_body) })
        .chain(chunks_stream)
        .take(MAX_HTTP_REQUEST_STREAM_CALLBACK_CALL_COUNT)
        .map(|x| async move { x })
        .buffered(STREAM_CALLBACK_BUFFER);

    ResponseBodyStream::new(Box::pin(chunks_to_frames(body_stream)))
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

            let range_header = ("Range".to_string(), format!("bytes={}-", next_chunk_begin));
            let mut updated_headers = stream_state.http_request.headers().to_vec();
            updated_headers.push(range_header.clone());
            let headers = updated_headers
                .iter()
                .map(|(name, value)| HeaderField(name.into(), value.into()))
                .collect::<Vec<HeaderField>>()
                .into_iter();
            let query_result = canister
                .http_request(
                    &stream_state.http_request.method(),
                    &stream_state.http_request.url(),
                    headers,
                    &stream_state.http_request.body(),
                    Some(&u16::from(MAX_VERIFICATION_VERSION)),
                )
                .call()
                .await;
            let agent_response = match query_result {
                Ok((response,)) => response,
                Err(e) => return Err(e),
            };
            let range_values = get_content_range_values(
                &agent_response.headers,
                stream_state.fetched_length,
                agent_response.body.len(),
            )?;
            if range_values.total_length != stream_state.total_length {
                return Err(AgentError::InvalidHttpResponse(format!(
                    "chunk total_length={} differs from the initial total_length={}",
                    range_values.total_length, stream_state.total_length
                )));
            }
            let new_bytes_begin = stream_state
                .fetched_length
                .saturating_sub(range_values.range_begin);
            let chunk_length = range_values
                .range_end
                .saturating_sub(stream_state.fetched_length)
                + 1;
            let current_fetched_length = stream_state.fetched_length + chunk_length;
            // Verify the chunk from the range response.
            if agent_response.streaming_strategy.is_some() {
                return Err(AgentError::InvalidHttpResponse(
                    "unexpected StreamingStrategy".to_string(),
                ));
            }

            let Ok(status_code) = StatusCode::from_u16(agent_response.status_code) else {
                return Err(AgentError::InvalidHttpResponse(format!(
                    "Invalid canister response status code: {}",
                    agent_response.status_code
                )));
            };
            let response = HttpResponse::builder()
                .with_status_code(status_code)
                .with_headers(
                    agent_response
                        .headers
                        .iter()
                        .map(|HeaderField(k, v)| (k.to_string(), v.to_string()))
                        .collect(),
                )
                .with_body(agent_response.body.clone())
                .build();
            let mut http_request = stream_state.http_request.clone();
            http_request.headers_mut().push(range_header);
            let validation_result = validate(
                &agent,
                &stream_state.canister_id,
                http_request,
                response,
                stream_state.skip_verification,
            );

            if let Err(e) = validation_result {
                return Err(AgentError::InvalidHttpResponse(format!(
                    "CertificateVerificationFailed for a chunk starting at {}, error: {}",
                    stream_state.fetched_length, e
                )));
            }
            let Some(new_bytes) = agent_response.body.get(new_bytes_begin..) else {
                return Err(AgentError::InvalidHttpResponse(format!(
                    "chunk starting at {} is shorter than expected",
                    stream_state.fetched_length
                )));
            };
            let maybe_new_state = if current_fetched_length < stream_state.total_length {
                Some(StreamState {
                    fetched_length: current_fetched_length,
                    ..stream_state
                })
            } else {
                None
            };
            Ok(Some((
                (new_bytes.to_vec(), maybe_new_state.clone()),
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
            let output = result.unwrap_or_else(|_| panic!("failed parsing '{}'", input));
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
            let result = parse_content_range_header_str(input);
            assert_matches!(result, Err(e) if format!("{}", e).contains("Invalid "));
        }
    }

    #[test]
    fn should_fail_parse_content_range_header_str_on_inconsistent_input() {
        let inconsistent_inputs = ["bytes 100-200/190", "bytes 200-150/400", "bytes 100-110/40"];
        for input in inconsistent_inputs {
            let result = parse_content_range_header_str(input);
            assert_matches!(result, Err(e) if format!("{}", e).contains("inconsistent Content-Range header"));
        }
    }

    #[test]
    fn should_get_initial_stream_state() {
        let http_request = HttpRequest::get("http://example.com/some_file")
            .with_headers(vec![("Xyz".to_string(), "some value".to_string())])
            .with_body(vec![42])
            .build();
        let canister_id = Principal::from_slice(&[1, 2, 3, 4]);
        let response_headers = vec![HeaderField(
            Cow::from("Content-Range"),
            Cow::from("bytes 0-2/10"), // fetched 3 bytes, total length is 10
        )];
        let skip_verification = false;
        let state = get_initial_stream_state(
            http_request.clone(),
            canister_id,
            &response_headers,
            3,
            skip_verification,
        )
        .expect("failed constructing StreamState");
        assert_eq!(state.http_request, http_request);
        assert_eq!(state.canister_id, canister_id);
        assert_eq!(state.fetched_length, 3);
        assert_eq!(state.total_length, 10);
        assert_eq!(state.skip_verification, skip_verification);
    }

    #[test]
    fn should_fail_get_initial_stream_state_without_content_range_header() {
        let http_request = HttpRequest::get("http://example.com/some_file")
            .with_headers(vec![("Xyz".to_string(), "some value".to_string())])
            .with_body(vec![42])
            .build();
        let canister_id = Principal::from_slice(&[1, 2, 3, 4]);
        let response_headers = vec![HeaderField(
            Cow::from("other header"),
            Cow::from("other value"),
        )];
        let result =
            get_initial_stream_state(http_request, canister_id, &response_headers, 3, false);
        assert_matches!(result, Err(e) if format!("{}", e).contains("missing Content-Range header"));
    }

    #[test]
    fn should_fail_get_initial_stream_state_with_malformed_content_range_header() {
        let http_request = HttpRequest::get("http://example.com/some_file")
            .with_headers(vec![("Xyz".to_string(), "some value".to_string())])
            .with_body(vec![42])
            .build();
        let canister_id = Principal::from_slice(&[1, 2, 3, 4]);
        let response_headers = vec![HeaderField(
            Cow::from("Content-Range"),
            Cow::from("bytes 42/10"),
        )];
        let result =
            get_initial_stream_state(http_request, canister_id, &response_headers, 3, false);
        assert_matches!(result, Err(e) if format!("{}", e).contains("Invalid bytes spec in Content-Range header"));
    }

    #[test]
    fn should_fail_get_initial_stream_state_with_inconsistent_content_range_header() {
        let http_request = HttpRequest::get("http://example.com/some_file")
            .with_headers(vec![("Xyz".to_string(), "some value".to_string())])
            .with_body(vec![42])
            .build();
        let canister_id = Principal::from_slice(&[1, 2, 3, 4]);
        let response_headers = vec![HeaderField(
            Cow::from("Content-Range"),
            Cow::from("bytes 40-100/90"),
        )];
        let result =
            get_initial_stream_state(http_request, canister_id, &response_headers, 61, false);
        assert_matches!(result, Err(e) if format!("{}", e).contains("inconsistent Content-Range header"));
    }

    #[test]
    fn should_fail_get_initial_stream_state_with_body_length_mismatch() {
        let http_request = HttpRequest::get("http://example.com/some_file").build();
        let canister_id = Principal::from_slice(&[1, 2, 3, 4]);
        let response_headers = vec![HeaderField(
            Cow::from("Content-Range"),
            Cow::from("bytes 0-99/1000"), // claims 100 bytes
        )];
        for body_length in [0, 10, 99, 101] {
            let result = get_initial_stream_state(
                http_request.clone(),
                canister_id,
                &response_headers,
                body_length,
                false,
            );
            assert_matches!(result, Err(e) if format!("{}", e).contains("does not match body length"));
        }
    }

    fn content_range_headers(value: &str) -> Vec<HeaderField<'static>> {
        vec![HeaderField(
            Cow::from("Content-Range"),
            Cow::from(value.to_string()),
        )]
    }

    #[test]
    fn should_get_content_range_values_when_body_length_matches() {
        // (Content-Range, fetched_length, body_length)
        let inputs = [
            ("bytes 0-9/1000", 0, 10),
            ("bytes 100-149/1000", 100, 50),
            ("bytes 90-149/1000", 100, 60), // overlaps with the already fetched bytes
            ("bytes 999-999/1000", 999, 1),
        ];
        for (header, fetched_length, body_length) in inputs {
            let result = get_content_range_values(
                &content_range_headers(header),
                fetched_length,
                body_length,
            );
            assert!(
                result.is_ok(),
                "unexpected error for '{header}': {result:?}"
            );
        }
    }

    #[test]
    fn should_fail_get_content_range_values_with_body_length_mismatch() {
        // (Content-Range, fetched_length, body_length)
        let inputs = [
            // The header claims a range starting before the already fetched bytes,
            // but the body is too short to contain them.
            ("bytes 0-149/1000", 100, 10),
            ("bytes 0-20/1000", 10, 5),
            ("bytes 90-149/1000", 100, 5),
            ("bytes 100-149/1000", 100, 0),
            ("bytes 100-149/1000", 100, 49),
            ("bytes 100-149/1000", 100, 51),
        ];
        for (header, fetched_length, body_length) in inputs {
            let result = get_content_range_values(
                &content_range_headers(header),
                fetched_length,
                body_length,
            );
            assert_matches!(result, Err(e) if format!("{}", e).contains("does not match body length"));
        }
    }

    fn collect_frames(chunks: Vec<Result<Vec<u8>, AgentError>>) -> Vec<ResponseBodyStreamItem> {
        futures::executor::block_on(chunks_to_frames(stream::iter(chunks)).collect::<Vec<_>>())
    }

    fn collect_frames_data(chunks: Vec<Result<Vec<u8>, AgentError>>) -> Vec<Bytes> {
        collect_frames(chunks)
            .into_iter()
            .map(|frame| {
                frame
                    .expect("unexpected error in body stream")
                    .into_data()
                    .expect("expected a DATA frame")
            })
            .collect()
    }

    #[test]
    fn should_skip_empty_chunks() {
        let data = collect_frames_data(vec![
            Ok(vec![]),
            Ok(vec![1, 2, 3]),
            Ok(vec![]),
            Ok(vec![]),
            Ok(vec![4, 5]),
            Ok(vec![]),
        ]);
        assert_eq!(
            data,
            vec![Bytes::from_static(&[1, 2, 3]), Bytes::from_static(&[4, 5])]
        );
    }

    #[test]
    fn should_yield_no_frames_for_empty_body() {
        let data = collect_frames_data(vec![Ok(vec![]), Ok(vec![])]);
        assert!(data.is_empty());
    }

    #[test]
    fn should_propagate_errors_when_skipping_empty_chunks() {
        let frames = collect_frames(vec![
            Ok(vec![1]),
            Ok(vec![]),
            Err(AgentError::InvalidHttpResponse("boom".to_string())),
        ]);
        assert_eq!(frames.len(), 2);
        assert_matches!(&frames[0], Ok(frame) if frame.data_ref() == Some(&Bytes::from_static(&[1])));
        assert_matches!(&frames[1], Err(AgentError::InvalidHttpResponse(msg)) if msg == "boom");
    }
}
