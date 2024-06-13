use crate::{
    HttpGatewayClientBuilder, HttpGatewayRequestArgs, HttpGatewayRequestBuilder,
    HttpGatewayRequestBuilderArgs,
};
use ic_agent::Agent;

#[derive(Clone)]
pub struct HttpGatewayClientArgs {
    pub agent: Agent,
}

#[derive(Clone)]
pub struct HttpGatewayClient {
    agent: Agent,
}

impl HttpGatewayClient {
    pub fn new(args: HttpGatewayClientArgs) -> Self {
        Self { agent: args.agent }
    }

    pub fn builder() -> HttpGatewayClientBuilder {
        Default::default()
    }

    pub fn request(&self, args: HttpGatewayRequestArgs) -> HttpGatewayRequestBuilder {
        HttpGatewayRequestBuilder::new(HttpGatewayRequestBuilderArgs {
            request_args: args,
            agent: &self.agent,
        })
    }
}
