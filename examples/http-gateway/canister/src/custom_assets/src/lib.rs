use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use ic_asset_certification::{Asset, AssetConfig, AssetFallbackConfig, AssetRouter};
use ic_cdk::{
    api::{data_certificate, set_certified_data},
    *,
};
use ic_certification::HashTree;
use ic_http_certification::{HeaderField, HttpCertificationTree, HttpRequest, HttpResponse};
use serde::Serialize;
use std::cell::RefCell;
use std::rc::Rc;

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
    serve_asset(&req)
}

thread_local! {
    static HTTP_TREE: Rc<RefCell<HttpCertificationTree>> = Default::default();

    // initializing the asset router with an HTTP certification tree is optional.
    // if direct access to the HTTP certification tree is not needed for certifying
    // requests and responses outside of the asset router, then this step can be skipped.
    static ASSET_ROUTER: RefCell<AssetRouter<'static>> = RefCell::new(AssetRouter::with_tree(HTTP_TREE.with(|tree| tree.clone())));
}

static ASSET_206_BODY: &[u8; 64] =
    b"<html><body>Some asset that returns a 206-response</body></html>";

fn certify_all_assets() {
    let asset_configs = vec![
        AssetConfig::File {
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
        },
        AssetConfig::File {
            path: "asset_206".to_string(),
            content_type: Some("text/html".to_string()),
            headers: get_asset_headers(vec![(
                "cache-control".to_string(),
                "public, no-cache, no-store".to_string(),
            )]),
            fallback_for: vec![],
            aliased_by: vec![],
            encodings: vec![],
        },
    ];

    let assets = vec![
        Asset::new("index.html", b"<html><body>Hello, world!</body></html>"),
        Asset::new("asset_206", ASSET_206_BODY),
    ];

    ASSET_ROUTER.with_borrow_mut(|asset_router| {
        if let Err(err) = asset_router.certify_assets(assets, asset_configs) {
            ic_cdk::trap(&format!("Failed to certify assets: {}", err));
        }

        set_certified_data(&asset_router.root_hash());
    });
}

fn serve_asset(req: &HttpRequest) -> HttpResponse<'static> {
    ASSET_ROUTER.with_borrow(|asset_router| {
        if let Ok((mut response, witness, expr_path)) = asset_router.serve_asset(req) {
            add_certificate_header(&mut response, &witness, &expr_path);
            // 'asset_206' is split into two chunks, to test "chunk-wise" serving of assets.
            if req.url().contains("asset_206") {
                const FIRST_CHUNK_LEN: usize = 42;
                let mut builder = HttpResponse::builder()
                    .with_status_code(206)
                    .with_headers(response.headers().to_vec())
                    .with_upgrade(response.upgrade().unwrap_or(false));
                let content_range = if req
                    .headers()
                    .contains(&("Range".to_string(), format!("bytes={}-", FIRST_CHUNK_LEN)))
                {
                    builder = builder.with_body(ASSET_206_BODY[FIRST_CHUNK_LEN..].to_vec());
                    format!(
                        "bytes {}-{}/{}",
                        FIRST_CHUNK_LEN,
                        ASSET_206_BODY.len() - 1,
                        ASSET_206_BODY.len()
                    )
                } else {
                    builder = builder.with_body(ASSET_206_BODY[..FIRST_CHUNK_LEN].to_vec());
                    format!("bytes 0-{}/{}", FIRST_CHUNK_LEN - 1, ASSET_206_BODY.len())
                };
                let mut response_206 = builder.build();
                response_206.add_header(("Content-Range".to_string(), content_range));
                response_206
            } else {
                response
            }
        } else {
            ic_cdk::trap("Failed to serve asset");
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

const IC_CERTIFICATE_HEADER: &str = "IC-Certificate";
fn add_certificate_header(response: &mut HttpResponse, witness: &HashTree, expr_path: &[String]) {
    let certified_data = data_certificate().expect("No data certificate available");
    let witness = cbor_encode(witness);
    let expr_path = cbor_encode(&expr_path);

    response.add_header((
        IC_CERTIFICATE_HEADER.to_string(),
        format!(
            "certificate=:{}:, tree=:{}:, expr_path=:{}:, version=2",
            BASE64.encode(certified_data),
            BASE64.encode(witness),
            BASE64.encode(expr_path)
        ),
    ));
}

fn cbor_encode(value: &impl Serialize) -> Vec<u8> {
    let mut serializer = serde_cbor::Serializer::new(Vec::new());
    serializer
        .self_describe()
        .expect("Failed to self describe CBOR");
    value
        .serialize(&mut serializer)
        .expect("Failed to serialize value");
    serializer.into_inner()
}
