# HTTP Gateway Protocol

## Overview

The HTTP Gateway Protocol is an extension of the Internet Computer Protocol that allows conventional HTTP clients to interact with the Internet Computer network. This is important for software such as web browsers to be able to fetch and render client-side canister code, including HTML, CSS, and JavaScript as well as other static assets such as images or videos. The HTTP Gateway does this by translating between standard HTTP requests and API canister calls that the Internet Computer Protocol will understand.

You can read more about this protocol in [the spec](https://github.com/dfinity/interface-spec/blob/master/spec/http-gateway-protocol-spec.md).

## Projects

### HTTP Canister Client

- [NPM Package](./packages/http-canister/README.md)
- [NodeJS Example](./examples/http-canister-client/nodejs/README.md)

| Command                                       | Description                    |
| --------------------------------------------- | ------------------------------ |
| `pnpm -F @dfinity/http-canister-client build` | Build NPM package              |
| `pnpm -F http-canister-nodejs-example start`  | Run the NodeJS example project |

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
- [Install NVM](https://github.com/nvm-sh/nvm)

Install the correct version of NodeJS:

```shell
nvm install
```

Activate the correct version of NodeJS:

```shell
nvm use
```

Install and activate the correct version of PNPM:

```shell
corepack enable
```

### Making a Commit

```shell
cz commit
```

See [Conventional commits](https://www.conventionalcommits.org/en/v1.0.0/) for more information on the commit message formats

### Package naming conventions

NPM packages are named `@dfinity/<package-name>` and the folder name is `<package-name>-js`.

### Referencing an NPM package

An NPM package can be referenced using the package name and [PNPM workspace protocol](https://pnpm.io/workspaces#workspace-protocol-workspace) in `package.json`:

```json
{
  "dependencies": {
    "@dfinity/certificate-verification": "workspace:*"
  }
}
```
