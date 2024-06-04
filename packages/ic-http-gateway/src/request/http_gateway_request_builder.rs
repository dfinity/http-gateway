use crate::{protocol::process_request, HttpGatewayResponse};
use candid::Principal;
use http::Request;
use ic_agent::Agent;

pub struct HttpGatewayRequestArgs {
    /// The request to make to the canister.
    pub canister_request: CanisterRequest,

    /// The id of the canister to make a request to.
    pub canister_id: Principal,
}

pub type CanisterRequest = Request<Vec<u8>>;

pub struct HttpGatewayRequestBuilderArgs<'a> {
    pub request_args: HttpGatewayRequestArgs,
    pub agent: &'a Agent,
}

pub struct HttpGatewayRequestBuilder<'a> {
    args: HttpGatewayRequestBuilderArgs<'a>,
    allow_skip_verification: bool,
}

impl<'a> HttpGatewayRequestBuilder<'a> {
    pub fn new(args: HttpGatewayRequestBuilderArgs<'a>) -> Self {
        Self {
            args,
            allow_skip_verification: false,
        }
    }

    pub fn unsafe_set_allow_skip_verification(
        &mut self,
        allow_skip_verification: bool,
    ) -> &mut Self {
        self.allow_skip_verification = allow_skip_verification;

        self
    }

    pub async fn send(self) -> HttpGatewayResponse {
        process_request(
            self.args.agent,
            self.args.request_args.canister_request,
            self.args.request_args.canister_id,
            self.allow_skip_verification,
        )
        .await
    }
}
