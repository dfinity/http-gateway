# HTTP Gateway Protocol

## Overview

The HTTP Gateway Protocol is an extension of the Internet Computer Protocol that allows conventional HTTP clients to interact with the Internet Computer network. This is important for software such as web browsers to be able to fetch and render client-side canister code, including HTML, CSS, and JavaScript as well as other static assets such as images or videos. The HTTP Gateway does this by translating between standard HTTP requests and API canister calls that the Internet Computer Protocol will understand.

You can read more about this protocol in [the spec](https://github.com/dfinity/interface-spec/blob/master/spec/http-gateway-protocol-spec.md).

## Packages

- [HTTP Canister Client](./packages/http-canister/README.md)

## Examples

- [HTTP Canister Client NodeJS](./examples/http-canister-client/nodejs/README.md)

## Related Projects

- [Response Verification](https://github.com/dfinity/response-verification/)
- [Service Worker](https://github.com/dfinity/ic/tree/master/typescript/service-worker)
- [ICX Proxy](https://github.com/dfinity/ic/tree/master/rs/boundary_node/icx_proxy)
- [Desktop HTTP Proxy](https://github.com/dfinity/http-proxy)

## Contributing

Check out our [contribution guidelines](./.github/CONTRIBUTING.md).

### Setup

- [Install pre-commit](https://pre-commit.com/#installation)
- [Install commitizen](https://commitizen-tools.github.io/commitizen/#installation)

### Making a Commit

```shell
cz commit
```

See [Conventional commits](https://www.conventionalcommits.org/en/v1.0.0/) for more information on the commit message formats.
