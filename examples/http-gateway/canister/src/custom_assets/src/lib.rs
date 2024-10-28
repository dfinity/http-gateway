use ic_asset_certification::{Asset, AssetConfig, AssetFallbackConfig, AssetRouter};
use ic_cdk::{
    api::{data_certificate, set_certified_data},
    *,
};
use ic_http_certification::{HeaderField, HttpRequest, HttpResponse, HttpResponseBuilder};
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
    let mut response = serve_asset(&req);
    if let Some(index) = chunk_corruption_requested(&req) {
        let current_chunk = current_chunk_index(&response);
        if current_chunk == index {
            // Create a response with a corrupted body.
            response = HttpResponseBuilder::new()
                .with_status_code(response.status_code())
                .with_headers(response.headers().to_vec())
                .with_upgrade(response.upgrade().unwrap_or(false))
                .with_body({
                    let mut body = response.body().to_vec();
                    body[0] += 1;
                    body
                })
                .build();
        }
    }
    if let Some(index) = cert_corruption_requested(&req) {
        let current_chunk = current_chunk_index(&response);
        if current_chunk == index {
            let body = response.body().to_owned();
            // Create a response with a corrupted certificate.
            response = HttpResponseBuilder::new()
                .with_status_code(response.status_code())
                .with_upgrade(response.upgrade().unwrap_or(false))
                .with_body(body)
                .with_headers({
                    let mut headers = response.headers().to_owned();
                    for (key, value) in headers.iter_mut() {
                        if key == "IC-Certificate" {
                            value.insert(15, char::from(42));
                        }
                    }
                    headers.to_vec()
                })
                .build();
        }
    }
    response
}

fn current_chunk_index(resp: &HttpResponse) -> usize {
    if let Some(content_range_header_value) = get_header_value(resp.headers(), "Content-Range") {
        get_content_range_begin(&content_range_header_value) / ASSET_CHUNK_SIZE
    } else {
        // Not a range-response, the asset is a single chunk
        0
    }
}

fn chunk_corruption_requested(req: &HttpRequest) -> Option<usize> {
    if let Some(corrupted_chunk_index) = get_header_value(req.headers(), "Test-CorruptedChunkIndex")
    {
        Some(
            corrupted_chunk_index
                .parse()
                .expect("invalid index of chunk to corrupt"),
        )
    } else {
        None
    }
}

fn cert_corruption_requested(req: &HttpRequest) -> Option<usize> {
    if let Some(corrupted_cert_chunk_index) =
        get_header_value(req.headers(), "Test-CorruptedCertificate")
    {
        Some(
            corrupted_cert_chunk_index
                .parse()
                .expect("invalid index of chunk to corrupt the certificate"),
        )
    } else {
        None
    }
}

fn get_header_value(headers: &[HeaderField], header_name: &str) -> Option<String> {
    for (name, value) in headers.iter() {
        if name.to_lowercase().eq(&header_name.to_lowercase()) {
            return Some(value.to_string());
        }
    }
    None
}

fn get_content_range_begin(content_range_header_value: &str) -> usize {
    // expected format: `bytes 21010-47021/47022`
    let re = regex::Regex::new(r"bytes\s+(\d+)-(\d+)/(\d+)").expect("invalid RE");
    let caps = re
        .captures(content_range_header_value)
        .expect("malformed Content-Range header");
    caps.get(1)
        .expect("missing range-begin")
        .as_str()
        .parse()
        .expect("malformed range-begin")
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
