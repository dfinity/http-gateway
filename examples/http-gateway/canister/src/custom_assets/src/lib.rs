use ic_asset_certification::{Asset, AssetConfig, AssetFallbackConfig, AssetRouter};
use ic_cdk::{
    api::{data_certificate, set_certified_data},
    *,
};
use ic_http_certification::{HeaderField, HttpRequest, HttpResponse};
use rand_chacha::rand_core::{RngCore, SeedableRng};
use rand_chacha::ChaCha20Rng;
use std::cell::RefCell;

#[init]
fn init() {
    certify_all_assets();
}

#[post_upgrade]
fn post_upgrade() {
    init();
}

#[query]
fn http_request(req: HttpRequest) -> HttpResponse {
    ic_cdk::println!("*** serving request ***: {:?}", req);
    let resp = serve_asset(&req);
    ic_cdk::println!(
        "\n+++ returning resp: {}, {:?}",
        resp.status_code(),
        resp.headers()
    );
    resp
}

thread_local! {
    static ASSET_ROUTER: RefCell<AssetRouter<'static>> = Default::default();
}

const ASSET_CHUNK_SIZE: usize = 2_000_000;

const ONE_CHUNK_ASSET_LEN: usize = ASSET_CHUNK_SIZE;
const TWO_CHUNKS_ASSET_LEN: usize = ASSET_CHUNK_SIZE + 1;
const SIX_CHUNKS_ASSET_LEN: usize = 5 * ASSET_CHUNK_SIZE + 12;
const TEN_CHUNKS_ASSET_LEN: usize = 10 * ASSET_CHUNK_SIZE;

const ONE_CHUNK_ASSET_NAME: &str = "long_asset_one_chunk";
const TWO_CHUNKS_ASSET_NAME: &str = "long_asset_two_chunks";
const SIX_CHUNKS_ASSET_NAME: &str = "long_asset_six_chunks";
const TEN_CHUNKS_ASSET_NAME: &str = "long_asset_ten_chunks";

use ic_certification::Hash;
use sha2::{Digest, Sha256};

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

fn certify_all_assets() {
    let mut assets = vec![Asset::new(
        "index.html",
        b"<html><body>Hello, world!</body></html>",
    )];
    let mut asset_configs = vec![AssetConfig::File {
        path: "index.html".to_string(),
        content_type: Some("text/html".to_string()),
        headers: get_asset_headers(vec![(
            "cache-control".to_string(),
            "public, no-cache, no-store".to_string(),
        )]),
        fallback_for: vec![AssetFallbackConfig {
            scope: "/".to_string(),
        }],
        aliased_by: vec!["/".to_string()],
        encodings: vec![],
    }];
    for asset_name in [
        ONE_CHUNK_ASSET_NAME,
        TWO_CHUNKS_ASSET_NAME,
        SIX_CHUNKS_ASSET_NAME,
        TEN_CHUNKS_ASSET_NAME,
    ] {
        asset_configs.push(AssetConfig::File {
            path: asset_name.to_string(),
            content_type: Some("application/octet-stream".to_string()),
            headers: get_asset_headers(vec![(
                "cache-control".to_string(),
                "public, no-cache, no-store".to_string(),
            )]),
            fallback_for: vec![],
            aliased_by: vec![],
            encodings: vec![],
        });
        assets.push(Asset::new(asset_name, long_asset_body(asset_name)));
    }

    ASSET_ROUTER.with_borrow_mut(|asset_router| {
        if let Err(err) = asset_router.certify_assets(assets, asset_configs) {
            ic_cdk::trap(&format!("Failed to certify assets: {}", err));
        }

        set_certified_data(&asset_router.root_hash());
    });
}

fn serve_asset(req: &HttpRequest) -> HttpResponse<'static> {
    ASSET_ROUTER.with_borrow(|asset_router| {
        if let Ok(response) = asset_router.serve_asset(
            &data_certificate().expect("No data certificate available"),
            req,
        ) {
            response
        } else {
            ic_cdk::trap(&format!("Failed to serve asset for request {:?}", req));
        }
    })
}

fn get_asset_headers(additional_headers: Vec<HeaderField>) -> Vec<HeaderField> {
    let mut headers = vec![
        ("strict-transport-security".to_string(), "max-age=31536000; includeSubDomains".to_string()),
        ("x-frame-options".to_string(), "DENY".to_string()),
        ("x-content-type-options".to_string(), "nosniff".to_string()),
        ("content-security-policy".to_string(), "default-src 'self'; form-action 'self'; object-src 'none'; frame-ancestors 'none'; upgrade-insecure-requests; block-all-mixed-content".to_string()),
        ("referrer-policy".to_string(), "no-referrer".to_string()),
        ("permissions-policy".to_string(), "accelerometer=(),ambient-light-sensor=(),autoplay=(),battery=(),camera=(),display-capture=(),document-domain=(),encrypted-media=(),fullscreen=(),gamepad=(),geolocation=(),gyroscope=(),layout-animations=(self),legacy-image-formats=(self),magnetometer=(),microphone=(),midi=(),oversized-images=(self),payment=(),picture-in-picture=(),publickey-credentials-get=(),speaker-selection=(),sync-xhr=(self),unoptimized-images=(self),unsized-media=(self),usb=(),screen-wake-lock=(),web-share=(),xr-spatial-tracking=()".to_string()),
        ("cross-origin-embedder-policy".to_string(), "require-corp".to_string()),
        ("cross-origin-opener-policy".to_string(), "same-origin".to_string()),
    ];
    headers.extend(additional_headers);

    headers
}
