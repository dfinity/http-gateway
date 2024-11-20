use http_body_util::BodyExt;
use hyper::{body::Incoming, server::conn::http2, service::service_fn, Request, Response};
use hyper_util::rt::TokioIo;
use ic_agent::Agent;
use ic_http_gateway::{HttpGatewayClient, HttpGatewayRequestArgs, HttpGatewayResponseBody};
use pocket_ic::PocketIcBuilder;
use std::{convert::Infallible, net::SocketAddr, path::PathBuf, sync::Arc};
use tokio::{fs::File, io::AsyncReadExt, net::TcpListener, task};

pub async fn load_custom_assets_wasm() -> Vec<u8> {
    load_wasm("http_gateway_canister_custom_assets").await
}

async fn load_wasm(canister: &str) -> Vec<u8> {
    let file_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../../.dfx/local/canisters")
        .join(canister)
        .join(format!("{}.wasm.gz", canister));

    load_file(file_path).await
}

async fn load_file(file_path: PathBuf) -> Vec<u8> {
    let mut file = File::open(&file_path).await.unwrap();

    let mut buffer = Vec::new();
    file.read_to_end(&mut buffer).await.unwrap();

    buffer
}

fn main() {
    let rt = tokio::runtime::Runtime::new().unwrap();
    let wasm_bytes = rt.block_on(async { load_custom_assets_wasm().await });

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

    rt.block_on(async {
        let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
        let listener = TcpListener::bind(addr).await.unwrap();

        println!("Listening on: {}", addr);

        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let io = TokioIo::new(stream);

            let http_gateway_clone = Arc::new(http_gateway.clone());

            let service = service_fn(move |req: Request<Incoming>| {
                let http_gateway_clone = Arc::clone(&http_gateway_clone);

                async move {
                    let canister_request = Request::builder().uri(req.uri()).method(req.method());
                    let collected_req = req.collect().await.unwrap().to_bytes();
                    let canister_request = canister_request.body(collected_req).unwrap();

                    let gateway_response = http_gateway_clone
                        .request(HttpGatewayRequestArgs {
                            canister_id,
                            canister_request,
                        })
                        .send()
                        .await;

                    Ok::<Response<HttpGatewayResponseBody>, Infallible>(
                        gateway_response.canister_response,
                    )
                }
            });

            let local = task::LocalSet::new();
            local
                .run_until(async move {
                    if let Err(err) = http2::Builder::new(LocalExec)
                        .serve_connection(io, service)
                        .await
                    {
                        eprintln!("Error serving connection: {:?}", err);
                    }
                })
                .await;
        }
    });
}

#[derive(Clone, Copy, Debug)]
struct LocalExec;

impl<F> hyper::rt::Executor<F> for LocalExec
where
    F: std::future::Future + 'static,
{
    fn execute(&self, fut: F) {
        tokio::task::spawn_local(fut);
    }
}
