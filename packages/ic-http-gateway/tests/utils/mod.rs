use std::path::PathBuf;
use tokio::{fs::File, io::AsyncReadExt};

pub async fn load_custom_assets_wasm() -> Vec<u8> {
    load_wasm("http_gateway_canister_custom_assets").await
}

async fn load_wasm(canister: &str) -> Vec<u8> {
    // First try to load from .dfx (if built locally with dfx)
    let dfx_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.dfx/local/canisters")
        .join(canister)
        .join(format!("{}.wasm.gz", canister));

    if dfx_path.exists() {
        return load_file(dfx_path).await;
    }

    // Fall back to pre-built WASM in examples directory
    let examples_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/http-gateway/canister")
        .join(format!("{}.wasm.gz", canister));

    load_file(examples_path).await
}

async fn load_file(file_path: PathBuf) -> Vec<u8> {
    let mut file = File::open(&file_path)
        .await
        .unwrap_or_else(|_| panic!("error opening file {:?}", file_path));

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).await.unwrap();

    buffer
}
