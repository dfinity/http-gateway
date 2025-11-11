use ic_asset_certification::{Asset, AssetConfig, AssetFallbackConfig, AssetRouter};
use ic_cdk::{
    api::{data_certificate, set_certified_data},
    *,
};
use ic_http_certification::{
    HeaderField, HttpRequest, HttpRequestBuilder, HttpResponse, HttpResponseBuilder, StatusCode,
};
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

// In addition to serving configured assets, the canister supports various
// corruption scenarios for testing purposes.  Specifically, the caller
// can use one of the following custom HTTP headers to make the canister
// "misbehave" in various ways:
// - "Test-CorruptChunkAtIndex"
// - "Test-CorruptCertificateAtIndex"
// - "Test-SwapChunkAtIndexWithNext"
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
    if let Some(chunk_to_swap) = chunk_swap_requested(&req) {
        let current_chunk = current_chunk_index(&response);
        if current_chunk == chunk_to_swap {
            // Create a request for the next chunk.
            let next_chunk_req = HttpRequestBuilder::new()
                .with_method(req.method().clone())
                .with_url(req.url())
                .with_body(req.body())
                .with_certificate_version(req.certificate_version().unwrap_or(2))
                .with_headers({
                    let mut headers = req.headers().to_owned();
                    let mut range_updated = false;
                    let new_range_value =
                        format!("bytes={}-", (chunk_to_swap + 1) * ASSET_CHUNK_SIZE);
                    for (key, value) in headers.iter_mut() {
                        if key == "Range" {
                            value.clear();
                            value.push_str(&new_range_value);
                            range_updated = true;
                        }
                    }
                    if !range_updated {
                        // The request had no Range-header, insert one.
                        headers.push(("Range".to_string(), new_range_value));
                    }
                    headers.to_vec()
                })
                .build();
            response = serve_asset(&next_chunk_req);
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
    get_header_value(req.headers(), "Test-CorruptChunkAtIndex").map(|corrupted_chunk_index| {
        corrupted_chunk_index
            .parse()
            .expect("invalid index of chunk to corrupt")
    })
}

fn cert_corruption_requested(req: &HttpRequest) -> Option<usize> {
    get_header_value(req.headers(), "Test-CorruptCertificateAtIndex").map(
        |corrupted_cert_chunk_index| {
            corrupted_cert_chunk_index
                .parse()
                .expect("invalid index of chunk to corrupt the certificate")
        },
    )
}

fn chunk_swap_requested(req: &HttpRequest) -> Option<usize> {
    get_header_value(req.headers(), "Test-SwapChunkAtIndexWithNext").map(|chunk_index_to_swap| {
        chunk_index_to_swap
            .parse()
            .expect("invalid index of chunk to swap")
    })
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
    let str_value = content_range_header_value.trim();
    if !str_value.starts_with("bytes ") {
        panic!(
            "Invalid Content-Range header: {}",
            content_range_header_value
        );
    }
    let str_value = str_value.trim_start_matches("bytes ");

    let str_value_parts = str_value.split('-').collect::<Vec<_>>();
    if str_value_parts.len() != 2 {
        panic!(
            "Invalid bytes spec in Content-Range header: {}",
            content_range_header_value
        );
    }
    let range_begin = str_value_parts[0]
        .parse::<usize>()
        .unwrap_or_else(|_| panic!("Invalid range_begin in: {content_range_header_value}"));

    // Note: skipping the check whether range_end and total_length are sane.
    range_begin
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
            status_code: Some(StatusCode::OK),
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
