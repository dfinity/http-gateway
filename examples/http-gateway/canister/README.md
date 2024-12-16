# An example canister

This is an example canister that serves various assets, demonstrating the usage
of the APIs from `ic-http-certification` and `ic-asset-certification` crates.
The canister is used for integration tests of `ic-http-gateway`-crate.
In addition to serving the assets it provides a mechanism for triggering
corrupted/malformed responses, which also are used in several tests.

`http_gateway_canister_custom_assets.wasm.gz` is is up-to-date WASM binary
of the canister, so that it can be used directly by external tests,
without re-building the canister.

To build the canister locally, run `dfx build http_gateway_canister_custom_assets`
in the main folder  of the repo, and find the binary at
`.dfx/local/canisters/http_gateway_canister_custom_assets/http_gateway_canister_custom_assets.wasm.gz`
