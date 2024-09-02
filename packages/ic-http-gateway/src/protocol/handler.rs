use super::validate;
use crate::consts::CONTENT_RANGE_HEADER_NAME;
use crate::{
    get_206_stream_response_body, get_body_and_streaming_body, CanisterRequest, CanisterResponse,
    HttpGatewayError, HttpGatewayResponse, HttpGatewayResponseBody, HttpGatewayResponseMetadata,
    HttpGatewayResult, ACCEPT_ENCODING_HEADER_NAME, CACHE_HEADER_NAME,
};
use candid::Principal;
use http::{Response, StatusCode};
use http_body_util::{BodyExt, Either, Full};
use ic_agent::{
    agent::{RejectCode, RejectResponse},
    Agent, AgentError,
};
use ic_http_certification::{HttpRequest, HttpResponse};
use ic_response_verification::MAX_VERIFICATION_VERSION;
use ic_utils::{
    call::{AsyncCall, SyncCall},
    interfaces::{http_request::HeaderField, HttpRequestCanister},
};

fn create_err_response(status_code: StatusCode, msg: &str) -> CanisterResponse {
    let mut response = Response::new(HttpGatewayResponseBody::Right(Full::from(
        msg.as_bytes().to_vec(),
    )));
    *response.status_mut() = status_code;

    response
}

fn convert_request(request: CanisterRequest) -> HttpGatewayResult<HttpRequest> {
    let uri = request.uri();
    let mut url = uri.path().to_string();
    if let Some(query) = uri.query() {
        url.push('?');
        url.push_str(query);
    }

    Ok(HttpRequest {
        method: request.method().to_string(),
        url,
        headers: request
            .headers()
            .into_iter()
            .map(|(name, value)| {
                Ok((
                    name.to_string(),
                    value
                        .to_str()
                        .map_err(|_| HttpGatewayError::HeaderValueParsingError {
                            header_name: name.to_string(),
                            header_value: String::from_utf8_lossy(value.as_bytes()).to_string(),
                        })?
                        .to_string(),
                ))
            })
            .collect::<HttpGatewayResult<Vec<_>>>()?,
        body: request.body().to_vec(),
    })
}

pub async fn process_request(
    agent: &Agent,
    request: CanisterRequest,
    canister_id: Principal,
    skip_verification: bool,
) -> HttpGatewayResponse {
    let http_request = match convert_request(request) {
        Ok(http_request) => http_request,
        Err(e) => {
            return HttpGatewayResponse {
                canister_response: create_err_response(
                    StatusCode::BAD_REQUEST,
                    &format!("Failed to parse request: {}", e),
                ),
                metadata: HttpGatewayResponseMetadata {
                    upgraded_to_update_call: false,
                    response_verification_version: None,
                    internal_error: Some(e),
                },
            }
        }
    };

    let canister = HttpRequestCanister::create(agent, canister_id);
    let header_fields = http_request
        .headers
        .iter()
        .filter(|(name, _)| name != "x-request-id")
        .map(|(name, value)| {
            if name.eq_ignore_ascii_case(ACCEPT_ENCODING_HEADER_NAME) {
                let mut encodings = value.split(',').map(|s| s.trim()).collect::<Vec<_>>();
                if !encodings.iter().any(|s| s.eq_ignore_ascii_case("identity")) {
                    encodings.push("identity");
                };

                let value = encodings.join(", ");
                return HeaderField(name.into(), value.into());
            }

            HeaderField(name.into(), value.into())
        })
        .collect::<Vec<HeaderField>>()
        .into_iter();

    let query_result = canister
        .http_request_custom(
            &http_request.method,
            &http_request.url,
            header_fields.clone(),
            &http_request.body,
            Some(&u16::from(MAX_VERIFICATION_VERSION)),
        )
        .call()
        .await;

    let agent_response = match query_result {
        Ok((response,)) => response,
        Err(e) => {
            return HttpGatewayResponse {
                canister_response: handle_agent_error(&e),
                metadata: HttpGatewayResponseMetadata {
                    upgraded_to_update_call: false,
                    response_verification_version: None,
                    internal_error: Some(e.into()),
                },
            };
        }
    };

    let is_update_call = agent_response.upgrade == Some(true);
    let agent_response = if is_update_call {
        let update_result = canister
            .http_request_update_custom(
                &http_request.method,
                &http_request.url,
                header_fields.clone(),
                &http_request.body,
            )
            .call_and_wait()
            .await;

        match update_result {
            Ok((response,)) => response,
            Err(e) => {
                return HttpGatewayResponse {
                    canister_response: handle_agent_error(&e),
                    metadata: HttpGatewayResponseMetadata {
                        upgraded_to_update_call: true,
                        response_verification_version: None,
                        internal_error: Some(e.into()),
                    },
                };
            }
        }
    } else {
        agent_response
    };

    let response_body = match get_body_and_streaming_body(agent, &agent_response).await {
        Ok(response_body) => response_body,
        Err(e) => {
            return HttpGatewayResponse {
                canister_response: create_err_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("Failed to parse response body: {}", e),
                ),
                metadata: HttpGatewayResponseMetadata {
                    upgraded_to_update_call: is_update_call,
                    response_verification_version: None,
                    internal_error: Some(e.into()),
                },
            }
        }
    };

    // There is no need to verify the response if the request was upgraded to an update call.
    // Also, we temporarily skip verification for partial (206) responses.
    // TODO: re-enable the verification of 206-responses once canister-code supports it.
    let validation_info = if !is_update_call && agent_response.status_code != 206 {
        // At the moment verification is only performed if the response is not using a streaming
        // strategy. Performing verification for those requests would required to join all the chunks
        // and this could cause memory issues and possibly create DOS attack vectors.
        match &response_body {
            Either::Right(body) => {
                // this unwrap should never panic because `Either::Right` will always have a full body
                let body = body.clone().collect().await.unwrap().to_bytes().to_vec();

                let validation_result = validate(
                    agent,
                    &canister_id,
                    http_request.clone(),
                    HttpResponse {
                        status_code: agent_response.status_code,
                        headers: agent_response
                            .headers
                            .iter()
                            .map(|HeaderField(k, v)| (k.to_string(), v.to_string()))
                            .collect(),
                        body,
                        upgrade: None,
                    },
                    skip_verification,
                );

                match validation_result {
                    Err(e) => {
                        return HttpGatewayResponse {
                            canister_response: create_err_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                &format!("Response verification failed: {}", e),
                            ),
                            metadata: HttpGatewayResponseMetadata {
                                upgraded_to_update_call: is_update_call,
                                response_verification_version: None,
                                internal_error: Some(e),
                            },
                        };
                    }
                    Ok(validation_info) => validation_info,
                }
            }
            _ => None,
        }
    } else {
        None
    };

    let response_verification_version = validation_info.as_ref().map(|e| e.verification_version);

    let status_code = match StatusCode::from_u16(agent_response.status_code) {
        Ok(status_code) => status_code,
        Err(e) => {
            return HttpGatewayResponse {
                canister_response: create_err_response(
                    StatusCode::BAD_REQUEST,
                    &format!("Failed to parse response status code: {}", e),
                ),
                metadata: HttpGatewayResponseMetadata {
                    upgraded_to_update_call: is_update_call,
                    response_verification_version,
                    internal_error: Some(e.into()),
                },
            }
        }
    };

    let mut response_builder = Response::builder().status(status_code);

    match &validation_info {
        // if there is no validation info, that means we've skipped verification,
        // this should only happen for raw domains, or for responses that are too
        // large to verify, return response as-is
        None => {
            for HeaderField(name, value) in &agent_response.headers {
                // Do not copy "Content-Range"-header, as clients obtain the full asset
                // via a streaming response.
                if !name.eq_ignore_ascii_case(CONTENT_RANGE_HEADER_NAME) {
                    response_builder = response_builder.header(name.as_ref(), value.as_ref());
                }
            }
        }

        Some(validation_info) => {
            if validation_info.verification_version < 2 {
                // status codes are not certified in v1, reject known dangerous status codes
                if agent_response.status_code >= 300 && agent_response.status_code < 400 {
                    return HttpGatewayResponse {
                        canister_response: create_err_response(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "Response verification v1 does not allow redirects",
                        ),
                        metadata: HttpGatewayResponseMetadata {
                            upgraded_to_update_call: is_update_call,
                            response_verification_version,
                            internal_error: None,
                        },
                    };
                }

                // headers are also not certified in v1, filter known dangerous headers
                for HeaderField(name, value) in &agent_response.headers {
                    if !name.eq_ignore_ascii_case(CACHE_HEADER_NAME) {
                        response_builder = response_builder.header(name.as_ref(), value.as_ref());
                    }
                }
            } else {
                match &validation_info.response {
                    // if there is no response, the canister has decided to certifiably skip verification,
                    // assume the developer knows what they're doing and return the response as-is
                    None => {
                        for HeaderField(name, value) in &agent_response.headers {
                            response_builder =
                                response_builder.header(name.as_ref(), value.as_ref());
                        }
                    }
                    // if there is a response, the canister has decided to certify some (but not necessarily all) headers,
                    // return only the certified headers
                    Some(certified_http_response) => {
                        for (name, value) in &certified_http_response.headers {
                            response_builder = response_builder.header(name, value);
                        }
                    }
                }
            }
        }
    }

    let response_body: HttpGatewayResponseBody = if status_code == 206 {
        // We got only the first chunk, turn the response into a streaming response
        match get_206_stream_response_body(
            agent,
            &http_request,
            canister_id,
            &agent_response.headers,
            response_body,
        )
        .await
        {
            Ok(stream_response_body) => stream_response_body,
            Err(e) => {
                return HttpGatewayResponse {
                    canister_response: create_err_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        &format!("Failed to create streaming responsey: {}", e),
                    ),
                    metadata: HttpGatewayResponseMetadata {
                        upgraded_to_update_call: is_update_call,
                        response_verification_version: None,
                        internal_error: Some(e.into()),
                    },
                }
            }
        }
    } else {
        response_body
    };

    let response = match response_builder.body(response_body) {
        Ok(response) => response,
        Err(e) => {
            return HttpGatewayResponse {
                canister_response: create_err_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    &format!("Failed to build response: {}", e),
                ),
                metadata: HttpGatewayResponseMetadata {
                    upgraded_to_update_call: is_update_call,
                    response_verification_version,
                    internal_error: Some(e.into()),
                },
            }
        }
    };

    HttpGatewayResponse {
        canister_response: response,
        metadata: HttpGatewayResponseMetadata {
            upgraded_to_update_call: is_update_call,
            response_verification_version,
            internal_error: None,
        },
    }
}

pub(crate) fn handle_agent_error(error: &AgentError) -> CanisterResponse {
    match error {
        // Turn all `DestinationInvalid`s into 404
        AgentError::CertifiedReject(RejectResponse {
            reject_code: RejectCode::DestinationInvalid,
            reject_message,
            ..
        }) => create_err_response(StatusCode::NOT_FOUND, reject_message),

        // If the result is a Replica error, returns the 500 code and message. There is no information
        // leak here because a user could use `dfx` to get the same reply.
        AgentError::CertifiedReject(response) => create_err_response(
            StatusCode::BAD_GATEWAY,
            &format!(
                "Replica Error: reject code {:?}, message {}, error code {:?}",
                response.reject_code, response.reject_message, response.error_code,
            ),
        ),

        AgentError::UncertifiedReject(RejectResponse {
            reject_code: RejectCode::DestinationInvalid,
            reject_message,
            ..
        }) => create_err_response(StatusCode::NOT_FOUND, reject_message),

        // If the result is a Replica error, returns the 500 code and message. There is no information
        // leak here because a user could use `dfx` to get the same reply.
        AgentError::UncertifiedReject(response) => create_err_response(
            StatusCode::BAD_GATEWAY,
            &format!(
                "Replica Error: reject code {:?}, message {}, error code {:?}",
                response.reject_code, response.reject_message, response.error_code,
            ),
        ),

        AgentError::ResponseSizeExceededLimit() => create_err_response(
            StatusCode::INSUFFICIENT_STORAGE,
            "Response size exceeds limit",
        ),

        // Handle all other errors
        _ => create_err_response(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Internal Server Error: {:?}", error),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http::Request;

    #[test]
    fn test_convert_request() {
        let request = Request::builder()
            .uri("http://example.com/foo/bar/baz?q=hello+world&t=1")
            .header("Accept", "text/html")
            .header("Accept-Encoding", "gzip, deflate, br, zstd")
            .body(b"body".to_vec())
            .unwrap();

        let http_request = convert_request(request).unwrap();

        assert_eq!(
            http_request,
            HttpRequest {
                method: "GET".to_string(),
                url: "/foo/bar/baz?q=hello+world&t=1".to_string(),
                headers: vec![
                    ("accept".to_string(), "text/html".to_string()),
                    (
                        "accept-encoding".to_string(),
                        "gzip, deflate, br, zstd".to_string()
                    ),
                ],
                body: b"body".to_vec(),
            }
        );
    }
}
