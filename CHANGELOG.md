## 0.1.0 (2024-12-20)

### Feat

- add a pre-built WASM of the example custom_assets canister (#34)

## 0.1.0-b3 (2024-11-20)

### Feat

- **ic-http-gateway**: use bytes for request body (#31)

## 0.1.0-b2 (2024-11-19)

### Fix

- remove dependency on regex to reduce WASM size (#29)

## 0.1.0-b1 (2024-10-30)

### Feat

- **ic-http-gateway**: add more tests and checks for long asset handling (#28)
- **ic-http-gateway**: enable validation of long assets' chunks (#24)
- **ic-http-gateway**: TT-416 Add asset streaming via range requests (#20)

### Fix

- **TT-409**: bubble up protocol errors to the client (#25)

## 0.1.0-b0 (2024-08-29)

### Feat

- **ic-http-gateway**: turn all errors into http responses
- **ic-http-gateway**: make error clonable
- **ic-http-gateway**: return internal error in response metadata
- **ic-http-gateway**: add compatibility with http-body crate
- **ic-http-gateway**: add initial ic-http-gateway library
- add http canister client
- init repo

### Fix

- **ic-http-gateway**: impl body trait correctly
- use correct value for 'upgraded_to_update_call'
- add missing exports to http-canister-client package
