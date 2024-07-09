use crate::HttpGatewayResult;
use candid::Principal;
use ic_agent::Agent;
use ic_http_certification::{HttpRequest, HttpResponse};
use ic_response_verification::{
    types::VerificationInfo, verify_request_response_pair, MIN_VERIFICATION_VERSION,
};
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_CERT_TIME_OFFSET_NS: u128 = 300_000_000_000;

pub fn validate(
    agent: &Agent,
    canister_id: &Principal,
    request: HttpRequest,
    response: HttpResponse,
    skip_verification: bool,
) -> HttpGatewayResult<Option<VerificationInfo>> {
    if skip_verification {
        // TODO: Remove this (FOLLOW-483)
        // Canisters don't have to provide certified variables
        // This should change in the future, grandfathering in current implementations
        return Ok(None);
    }

    let ic_public_key = agent.read_root_key();
    let verification_info = verify_request_response_pair(
        request,
        response,
        canister_id.as_slice(),
        get_current_time_in_ns(),
        MAX_CERT_TIME_OFFSET_NS,
        ic_public_key.as_slice(),
        MIN_VERIFICATION_VERSION,
    )?;
    Ok(Some(verification_info))
}

fn get_current_time_in_ns() -> u128 {
    let start = SystemTime::now();

    start
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_nanos()
}
