use super::validate;
use crate::{
    get_body_and_streaming_body, CanisterRequest, CanisterResponse, HttpGatewayError,
    HttpGatewayResponse, HttpGatewayResponseBody, HttpGatewayResponseMetadata, HttpGatewayResult,
    ACCEPT_ENCODING_HEADER_NAME, CACHE_HEADER_NAME,
};
use candid::Principal;
use http::{Response, StatusCode};
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

fn convert_request(request: CanisterRequest) -> HttpGatewayResult<HttpRequest> {
    Ok(HttpRequest {
        method: request.method().to_string(),
        url: request.uri().to_string(),
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
                            header_value: value.as_bytes().to_vec(),
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
    allow_skip_verification: bool,
) -> HttpGatewayResult<HttpGatewayResponse<'_>> {
    let http_request = convert_request(request)?;

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
            let err_res = handle_agent_error(e)?;

            return Ok(HttpGatewayResponse {
                canister_response: err_res,
                metadata: HttpGatewayResponseMetadata {
                    upgraded_to_update_call: false,
                    response_verification_version: None,
                },
            });
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
                let err_res = handle_agent_error(e)?;

                return Ok(HttpGatewayResponse {
                    canister_response: err_res,
                    metadata: HttpGatewayResponseMetadata {
                        upgraded_to_update_call: false,
                        response_verification_version: None,
                    },
                });
            }
        }
    } else {
        agent_response
    };

    let response_body = get_body_and_streaming_body(agent, &agent_response).await?;

    // there is no need to verify the response if the request was upgraded to an update call
    let validation_info = if !is_update_call {
        // At the moment verification is only performed if the response is not using a streaming
        // strategy. Performing verification for those requests would required to join all the chunks
        // and this could cause memory issues and possibly create DOS attack vectors.
        match &response_body {
            HttpGatewayResponseBody::Bytes(body) => {
                let validation_result = validate(
                    agent,
                    &canister_id,
                    http_request,
                    HttpResponse {
                        status_code: agent_response.status_code,
                        headers: agent_response
                            .headers
                            .iter()
                            .map(|HeaderField(k, v)| (k.to_string(), v.to_string()))
                            .collect(),
                        body: body.to_owned(),
                        upgrade: None,
                    },
                    allow_skip_verification,
                );

                match validation_result {
                    Err(err) => {
                        return Ok(HttpGatewayResponse {
                            canister_response: Response::builder()
                                .status(StatusCode::INTERNAL_SERVER_ERROR)
                                .body(HttpGatewayResponseBody::Bytes(err.as_bytes().to_vec()))
                                .unwrap(),
                            metadata: HttpGatewayResponseMetadata {
                                upgraded_to_update_call: is_update_call,
                                response_verification_version: None,
                            },
                        });
                    }
                    Ok(validation_info) => validation_info,
                }
            }
            _ => None,
        }
    } else {
        None
    };

    let mut response_builder =
        Response::builder().status(StatusCode::from_u16(agent_response.status_code)?);

    match &validation_info {
        // if there is no validation info, that means we've skipped verification,
        // this should only happen for raw domains,
        // return response as-is
        None => {
            for HeaderField(name, value) in &agent_response.headers {
                response_builder = response_builder.header(name.as_ref(), value.as_ref());
            }
        }

        Some(validation_info) => {
            if validation_info.verification_version < 2 {
                // status codes are not certified in v1, reject known dangerous status codes
                if agent_response.status_code >= 300 && agent_response.status_code < 400 {
                    let msg = b"Response verification v1 does not allow redirects";

                    return Ok(HttpGatewayResponse {
                        canister_response: Response::builder()
                            .status(StatusCode::INTERNAL_SERVER_ERROR)
                            .body(HttpGatewayResponseBody::Bytes(msg.to_vec()))
                            .unwrap(),
                        metadata: HttpGatewayResponseMetadata {
                            upgraded_to_update_call: is_update_call,
                            response_verification_version: Some(
                                validation_info.verification_version,
                            ),
                        },
                    });
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

    let response = response_builder.body(response_body)?;

    return Ok(HttpGatewayResponse {
        canister_response: response,
        metadata: HttpGatewayResponseMetadata {
            upgraded_to_update_call: is_update_call,
            response_verification_version: validation_info.map(|e| e.verification_version),
        },
    });
}

fn handle_agent_error<'a>(error: AgentError) -> HttpGatewayResult<CanisterResponse<'a>> {
    match error {
        // Turn all `DestinationInvalid`s into 404
        AgentError::CertifiedReject(RejectResponse {
            reject_code: RejectCode::DestinationInvalid,
            reject_message,
            ..
        }) => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(HttpGatewayResponseBody::Bytes(
                reject_message.as_bytes().to_vec(),
            ))
            .unwrap()),

        // If the result is a Replica error, returns the 500 code and message. There is no information
        // leak here because a user could use `dfx` to get the same reply.
        AgentError::CertifiedReject(response) => {
            let msg = format!(
                "Replica Error: reject code {:?}, message {}, error code {:?}",
                response.reject_code, response.reject_message, response.error_code,
            );

            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(HttpGatewayResponseBody::Bytes(msg.as_bytes().to_vec()))
                .unwrap())
        }

        AgentError::UncertifiedReject(RejectResponse {
            reject_code: RejectCode::DestinationInvalid,
            reject_message,
            ..
        }) => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(HttpGatewayResponseBody::Bytes(
                reject_message.as_bytes().to_vec(),
            ))
            .unwrap()),

        // If the result is a Replica error, returns the 500 code and message. There is no information
        // leak here because a user could use `dfx` to get the same reply.
        AgentError::UncertifiedReject(response) => {
            let msg = format!(
                "Replica Error: reject code {:?}, message {}, error code {:?}",
                response.reject_code, response.reject_message, response.error_code,
            );

            Ok(Response::builder()
                .status(StatusCode::BAD_GATEWAY)
                .body(HttpGatewayResponseBody::Bytes(msg.as_bytes().to_vec()))
                .unwrap())
        }

        AgentError::ResponseSizeExceededLimit() => Ok(Response::builder()
            .status(StatusCode::INSUFFICIENT_STORAGE)
            .body(HttpGatewayResponseBody::Bytes(
                b"Response size exceeds limit".to_vec(),
            ))
            .unwrap()),

        // Handle all other errors
        e => Err(e.into()),
    }
}
