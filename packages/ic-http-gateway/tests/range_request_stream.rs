use http::Request;
use http_body_util::BodyExt;
use ic_agent::hash_tree::Hash;
use ic_agent::Agent;
use ic_http_gateway::{HttpGatewayClient, HttpGatewayRequestArgs, HttpGatewayResponseMetadata};
use pocket_ic::PocketIcBuilder;
use rand_chacha::rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use rstest::*;
use sha2::{Digest, Sha256};
use std::cmp::min;

mod utils;

const ASSET_CHUNK_SIZE: usize = 2_000_000;

const ONE_CHUNK_ASSET_LEN: usize = ASSET_CHUNK_SIZE;
const TWO_CHUNKS_ASSET_LEN: usize = ASSET_CHUNK_SIZE + 1;
const SIX_CHUNKS_ASSET_LEN: usize = 5 * ASSET_CHUNK_SIZE + 12;
const TEN_CHUNKS_ASSET_LEN: usize = 10 * ASSET_CHUNK_SIZE;

const ONE_CHUNK_ASSET_NAME: &str = "long_asset_one_chunk";
const TWO_CHUNKS_ASSET_NAME: &str = "long_asset_two_chunks";
const SIX_CHUNKS_ASSET_NAME: &str = "long_asset_six_chunks";
const TEN_CHUNKS_ASSET_NAME: &str = "long_asset_ten_chunks";

pub fn hash<T>(data: T) -> Hash
where
    T: AsRef<[u8]>,
{
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

fn long_asset_body(asset_name: &str) -> Vec<u8> {
    let asset_length = match asset_name {
        ONE_CHUNK_ASSET_NAME => ONE_CHUNK_ASSET_LEN,
        TWO_CHUNKS_ASSET_NAME => TWO_CHUNKS_ASSET_LEN,
        SIX_CHUNKS_ASSET_NAME => SIX_CHUNKS_ASSET_LEN,
        TEN_CHUNKS_ASSET_NAME => TEN_CHUNKS_ASSET_LEN,
        _ => ASSET_CHUNK_SIZE * 3 + 1,
    };
    let mut rng = ChaCha20Rng::from_seed(hash(asset_name));
    let mut body = vec![0u8; asset_length];
    rng.fill_bytes(&mut body);
    body
}

#[rstest]
#[case("index.html")]
#[case(TWO_CHUNKS_ASSET_NAME)]
fn test_long_asset_yields_range_request_stream(#[case] asset_name: &str) {
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
                canister_request: Request::builder()
                    .uri(format!("/{asset_name}"))
                    .body(vec![])
                    .unwrap(),
            })
            .send()
            .await
    });
    println!(
        "--- response for {} is {:?}",
        asset_name,
        response.canister_response.headers()
    );

    let response_headers = response
        .canister_response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
        .collect::<Vec<(&str, &str)>>();

    assert_eq!(response.canister_response.status(), 206);

    // check that the response contains the certificate headers
    assert!(
        contains_header("ic-certificate", response_headers.clone()),
        "response does not contain 'ic-certificate' header"
    );

    assert!(
        contains_header("ic-certificateexpression", response_headers.clone()),
        "response does not contain 'ic-certificateexpression' header"
    );

    // remove certificate headers before checking the certified headers
    let certified_headers: Vec<(&str, &str)> = response_headers
        .iter()
        .filter(|(key, _)| *key != "ic-certificate" && *key != "ic-certificateexpression")
        .cloned() // To convert from iterator of references to an iterator of owned values
        .collect();

    let expected_body = long_asset_body(asset_name);

    assert_eq!(
        certified_headers,
        vec![
            ("strict-transport-security", "max-age=31536000; includeSubDomains"),
            ("x-frame-options", "DENY"),
            ("x-content-type-options", "nosniff"),
            ("content-security-policy", "default-src 'self'; form-action 'self'; object-src 'none'; frame-ancestors 'none'; upgrade-insecure-requests; block-all-mixed-content"),
            ("referrer-policy", "no-referrer"),
            ("permissions-policy", "accelerometer=(),ambient-light-sensor=(),autoplay=(),battery=(),camera=(),display-capture=(),document-domain=(),encrypted-media=(),fullscreen=(),gamepad=(),geolocation=(),gyroscope=(),layout-animations=(self),legacy-image-formats=(self),magnetometer=(),microphone=(),midi=(),oversized-images=(self),payment=(),picture-in-picture=(),publickey-credentials-get=(),speaker-selection=(),sync-xhr=(self),unoptimized-images=(self),unsized-media=(self),usb=(),screen-wake-lock=(),web-share=(),xr-spatial-tracking=()"),
            ("cross-origin-embedder-policy", "require-corp"),
            ("cross-origin-opener-policy", "same-origin"),
            ("cache-control", "public, no-cache, no-store"),
            ("content-type", "application/octet-stream"),
            ("content-length", expected_body.len().to_string().as_str()),
        ]
    );

    rt.block_on(async {
        let body = response
            .canister_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();

        assert_eq!(body, expected_body);
    });

    assert_response_metadata(
        response.metadata,
        HttpGatewayResponseMetadata {
            upgraded_to_update_call: false,
            response_verification_version: None,
            internal_error: None,
        },
    );
}

#[rstest]
#[case(ONE_CHUNK_ASSET_NAME)]
#[case(TWO_CHUNKS_ASSET_NAME)]
fn test_range_request_yields_range_response(#[case] asset_name: &str) {
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
                canister_request: Request::builder()
                    .uri(format!("/{asset_name}"))
                    .header("Range", format!("bytes={}-", ASSET_CHUNK_SIZE))
                    .body(vec![])
                    .unwrap(),
            })
            .send()
            .await
    });

    let expected_full_body = long_asset_body(asset_name);
    let expected_response_body = &expected_full_body[ASSET_CHUNK_SIZE..];
    let response_headers = response
        .canister_response
        .headers()
        .iter()
        .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
        .collect::<Vec<(&str, &str)>>();

    assert_eq!(response.canister_response.status(), 206);

    // check that the response contains the certificate headers
    assert!(
        contains_header("ic-certificate", response_headers.clone()),
        "response does not contain 'ic-certificate' header"
    );

    assert!(
        contains_header("ic-certificateexpression", response_headers.clone()),
        "response does not contain 'ic-certificateexpression' header"
    );

    // remove certificate headers before checking the certified headers
    let certified_headers: Vec<(&str, &str)> = response_headers
        .iter()
        .filter(|(key, _)| *key != "ic-certificate" && *key != "ic-certificateexpression")
        .cloned() // To convert from iterator of references to an iterator of owned values
        .collect();

    assert_eq!(
        certified_headers,
        vec![
            ("strict-transport-security", "max-age=31536000; includeSubDomains"),
            ("x-frame-options", "DENY"),
            ("x-content-type-options", "nosniff"),
            ("content-security-policy", "default-src 'self'; form-action 'self'; object-src 'none'; frame-ancestors 'none'; upgrade-insecure-requests; block-all-mixed-content"),
            ("referrer-policy", "no-referrer"),
            ("permissions-policy", "accelerometer=(),ambient-light-sensor=(),autoplay=(),battery=(),camera=(),display-capture=(),document-domain=(),encrypted-media=(),fullscreen=(),gamepad=(),geolocation=(),gyroscope=(),layout-animations=(self),legacy-image-formats=(self),magnetometer=(),microphone=(),midi=(),oversized-images=(self),payment=(),picture-in-picture=(),publickey-credentials-get=(),speaker-selection=(),sync-xhr=(self),unoptimized-images=(self),unsized-media=(self),usb=(),screen-wake-lock=(),web-share=(),xr-spatial-tracking=()"),
            ("cross-origin-embedder-policy", "require-corp"),
            ("cross-origin-opener-policy", "same-origin"),
            ("cache-control", "public, no-cache, no-store"),
            ("content-type", "application/octet-stream"),
            ("content-length", expected_response_body.len().to_string().as_str()),
            ("content-range", &format!(
                "bytes {}-{}/{}",
                ASSET_CHUNK_SIZE,
                min(expected_full_body.len(), 2*ASSET_CHUNK_SIZE) - 1,
                expected_full_body.len()
            ))
        ]
    );

    rt.block_on(async {
        let body = response
            .canister_response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec();

        assert_eq!(body, expected_response_body);
    });

    assert_response_metadata(
        response.metadata,
        HttpGatewayResponseMetadata {
            upgraded_to_update_call: false,
            response_verification_version: None,
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

fn contains_header(header_name: &str, headers: Vec<(&str, &str)>) -> bool {
    headers.iter().any(|(key, _)| *key == header_name)
}
