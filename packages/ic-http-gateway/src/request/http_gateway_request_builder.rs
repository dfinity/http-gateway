use crate::{protocol::process_request, HttpGatewayResponse};
use bytes::Bytes;
use candid::Principal;
use http::Request;
use ic_agent::Agent;

pub struct HttpGatewayRequestArgs {
    /// The request to make to the canister.
    pub canister_request: CanisterRequest,

    /// The id of the canister to make a request to.
    pub canister_id: Principal,
}

pub type CanisterRequest = Request<Bytes>;

pub struct HttpGatewayRequestBuilderArgs<'a> {
    pub request_args: HttpGatewayRequestArgs,
    pub agent: &'a Agent,
}

pub struct HttpGatewayRequestBuilder<'a> {
    args: HttpGatewayRequestBuilderArgs<'a>,
    skip_verification: bool,
}

impl<'a> HttpGatewayRequestBuilder<'a> {
    pub fn new(args: HttpGatewayRequestBuilderArgs<'a>) -> Self {
        Self {
            args,
            skip_verification: false,
        }
    }

    pub fn unsafe_set_skip_verification(&mut self, skip_verification: bool) -> &mut Self {
        self.skip_verification = skip_verification;

        self
    }

    pub async fn send(self) -> HttpGatewayResponse {
        process_request(
            self.args.agent,
            self.args.request_args.canister_request,
            self.args.request_args.canister_id,
            self.skip_verification,
        )
        .await
    }
}
