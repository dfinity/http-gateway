use crate::{HttpGatewayResponseBody, ResponseBodyStream};
use futures::{stream, Stream, StreamExt, TryStreamExt};
use ic_agent::{Agent, AgentError};
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

pub async fn get_body_and_streaming_body<'a, 'b>(
    agent: &'a Agent,
    response: &'b AgentResponseAny,
) -> Result<HttpGatewayResponseBody<'a>, AgentError> {
    // if we already have the full body, we can return it early
    let Some(StreamingStrategy::Callback(callback_strategy)) = response.streaming_strategy.clone()
    else {
        return Ok(HttpGatewayResponseBody::Bytes(response.body.clone()));
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

        return Ok(HttpGatewayResponseBody::Stream(body_stream));
    };

    // if we no longer have a token at this point,
    // we were able to collect the response within the allow certified callback limit,
    // return this collected response as a standard response body so it will be verified
    Ok(HttpGatewayResponseBody::Bytes(streamed_body))
}

fn create_body_stream<'a>(
    agent: Agent,
    callback: HttpRequestStreamingCallbackAny,
    token: Option<Token>,
    initial_body: Vec<u8>,
) -> ResponseBodyStream<'a> {
    let chunks_stream =
        create_stream(agent, callback, token).map(|chunk| chunk.map(|(body, _)| body));

    let body_stream = stream::once(async move { Ok(initial_body) })
        .chain(chunks_stream)
        .take(MAX_HTTP_REQUEST_STREAM_CALLBACK_CALL_COUNT)
        .map(|x| async move { x })
        .buffered(STREAM_CALLBACK_BUFFER);

    ResponseBodyStream::new(body_stream)
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
