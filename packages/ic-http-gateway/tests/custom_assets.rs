use http::Request;
use ic_agent::Agent;
use ic_http_gateway::{
    HttpGatewayClient, HttpGatewayRequestArgs, HttpGatewayResponseBody, HttpGatewayResponseMetadata,
};
use pocket_ic::PocketIcBuilder;

mod utils;

#[test]
fn test_custom_assets_index_html() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let wasm_bytes = rt.block_on(async { utils::load_custom_assets_wasm().await });

    let pic = PocketIcBuilder::new()
        .with_nns_subnet()
        .with_application_subnet()
        .build();

    let canister_id = pic.create_canister();
    pic.add_cycles(canister_id, 2_000_000_000_000);
    pic.install_canister(canister_id, wasm_bytes, vec![], None);

    let url = pic.auto_progress();

    let agent = Agent::builder().with_url(url).build().unwrap();
    rt.block_on(async {
        agent.fetch_root_key().await.unwrap();
    });

    let http_gateway = HttpGatewayClient::builder()
        .with_agent(agent)
        .build()
        .unwrap();

    let response = rt.block_on(async {
        http_gateway
            .request(HttpGatewayRequestArgs {
                canister_id,
                canister_request: Request::builder().uri("/").body(vec![]).unwrap(),
            })
            .send()
            .await
            .unwrap()
    });

    let index_html = b"<html><body>Hello, world!</body></html>";
    let response_headers = response
        .canister_response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
        .collect::<Vec<(&str, &str)>>();

    assert_eq!(response.canister_response.status(), 200);
    assert_eq!(
        response_headers,
        vec![
            ("content-length", index_html.len().to_string().as_str()),
            ("strict-transport-security", "max-age=31536000; includeSubDomains"),
            ("x-frame-options", "DENY"),
            ("x-content-type-options", "nosniff"),
            ("content-security-policy", "default-src 'self'; form-action 'self'; object-src 'none'; frame-ancestors 'none'; upgrade-insecure-requests; block-all-mixed-content"),
            ("referrer-policy", "no-referrer"),
            ("permissions-policy", "accelerometer=(),ambient-light-sensor=(),autoplay=(),battery=(),camera=(),display-capture=(),document-domain=(),encrypted-media=(),fullscreen=(),gamepad=(),geolocation=(),gyroscope=(),layout-animations=(self),legacy-image-formats=(self),magnetometer=(),microphone=(),midi=(),oversized-images=(self),payment=(),picture-in-picture=(),publickey-credentials-get=(),speaker-selection=(),sync-xhr=(self),unoptimized-images=(self),unsized-media=(self),usb=(),screen-wake-lock=(),web-share=(),xr-spatial-tracking=()"),
            ("cross-origin-embedder-policy", "require-corp"),
            ("cross-origin-opener-policy", "same-origin"),
            ("cache-control", "public, no-cache, no-store"),
            ("content-type", "text/html"),
        ]
    );
    matches!(
        response.canister_response.body(),
        HttpGatewayResponseBody::Bytes(body) if body == index_html
    );

    assert_response_metadata(
        response.metadata,
        HttpGatewayResponseMetadata {
            upgraded_to_update_call: false,
            response_verification_version: Some(2),
            internal_error: None,
        },
    );
}

fn assert_response_metadata(
    response_metadata: HttpGatewayResponseMetadata,
    expected_response_metadata: HttpGatewayResponseMetadata,
) {
    assert_eq!(
        response_metadata.upgraded_to_update_call,
        expected_response_metadata.upgraded_to_update_call
    );
    assert_eq!(
        response_metadata.response_verification_version,
        expected_response_metadata.response_verification_version
    );
}
