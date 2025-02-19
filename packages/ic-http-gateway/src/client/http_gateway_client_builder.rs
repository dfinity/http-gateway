use crate::{HttpGatewayClient, HttpGatewayClientArgs, HttpGatewayResult, DEFAULT_API_GATEWAY};
use ic_agent::Agent;

pub struct HttpGatewayClientBuilder {
    agent: Option<Agent>,
}

impl HttpGatewayClientBuilder {
    pub fn new() -> Self {
        Self { agent: None }
    }

    pub fn with_agent(mut self, agent: Agent) -> Self {
        self.agent = Some(agent);

        self
    }

    pub fn build(self) -> HttpGatewayResult<HttpGatewayClient> {
        let agent = match self.agent {
            Some(agent) => agent,
            None => Agent::builder().with_url(DEFAULT_API_GATEWAY).build()?,
        };

        Ok(HttpGatewayClient::new(HttpGatewayClientArgs { agent }))
    }
}

impl Default for HttpGatewayClientBuilder {
    fn default() -> Self {
        Self::new()
    }
}
