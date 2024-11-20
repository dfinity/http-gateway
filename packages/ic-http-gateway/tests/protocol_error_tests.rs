use bytes::Bytes;
use http::{status::StatusCode, Request};
use ic_agent::{export::Principal, Agent};
use ic_http_gateway::{HttpGatewayClient, HttpGatewayRequestArgs};
use reqwest::Client;
use std::{env, error::Error, process::Command, str::FromStr};
use testcontainers::{
    core::{IntoContainerPort, WaitFor},
    runners::AsyncRunner,
    GenericImage,
};

const IMAGE_NAME: &str = "ic-mock-busy-replica";
const IMAGE_TAG: &str = "latest";

fn build_gateway_image() -> Result<(), Box<dyn Error>> {
    let cwd = env::var("CARGO_MANIFEST_DIR")?;

    let output = Command::new("docker")
        .current_dir(format!("{cwd}/test-container"))
        .arg("build")
        .arg("--file")
        .arg("Dockerfile")
        .arg("--force-rm")
        .arg("--tag")
        .arg(format!("{IMAGE_NAME}:{IMAGE_TAG}"))
        .arg(".")
        .output()?;

    if !output.status.success() {
        eprintln!("stderr: {}", String::from_utf8(output.stderr)?);
        return Err("unable to build mock busy replica image.".into());
    }

    Ok(())
}

#[tokio::test]
async fn test_rate_limiting_error() -> Result<(), Box<dyn std::error::Error>> {
    build_gateway_image()?;

    // run the mock backend container
    let container = GenericImage::new(IMAGE_NAME, IMAGE_TAG)
        .with_exposed_port(8000.tcp())
        .with_wait_for(WaitFor::healthcheck())
        .start()
        .await?;

    // Retrieve the mapped port
    let backend_port = container.get_host_port_ipv4(8000).await?;
    let backend_host = container.get_host().await?.to_string();

    // Check that the mock canister is up
    let backend_base_url = format!("http://{}:{}", backend_host, backend_port);
    let healthcheck_url = format!("{}/healthcheck", backend_base_url);
    let response = Client::new().get(&healthcheck_url).send().await?;
    assert_eq!(
        response.status().as_u16(),
        200,
        "Expected to receive 200 from /healthcheck but received {}",
        response.status().as_u16()
    );

    // Make a gateway
    let agent = Agent::builder().with_url(backend_base_url).build().unwrap();
    let http_gateway = HttpGatewayClient::builder()
        .with_agent(agent)
        .build()
        .unwrap();

    // Fake a `GET /example` request coming into the gateway
    let canister_request = Request::builder()
        .uri("/example")
        .method("GET")
        .body(Bytes::new())
        .unwrap();

    let gateway_response = http_gateway
        .request(HttpGatewayRequestArgs {
            canister_id: Principal::from_str("qoctq-giaaa-aaaaa-aaaea-cai")?,
            canister_request,
        })
        .send()
        .await;

    assert_eq!(
        gateway_response.canister_response.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "Expected to receive a 429 from the node but received {}",
        gateway_response.canister_response.status()
    );

    Ok(())
}
