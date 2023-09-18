# HTTP Canister Client

A JavaScript client for Internet Computer canisters that implement the HTTP interface.

## Installation

Install the main package:

```shell
npm i @dfinity/http-canister-client
```

Install peer dependencies if you don't already have them:

```shell
npm i @dfinity/{agent,candid,principal}
```

## Usage

### HTTP Request

Making a query call to a canister's `http_request` method:

```typescript
import { HttpAgent } from '@dfinity/agent';
import { HttpCanisterClient } from '@dfinity/http-canister-client';

const canisterId = 'qoctq-giaaa-aaaaa-aaaea-cai';

const agent = new HttpAgent();
const client = new HttpCanisterClient(canisterId, agent);

const response = await client.httpRequest({
  url: '/',
  method: 'GET',
  body: [],
  headers: [['accept-encoding', 'gzip']],
  certificate_version: [],
});

console.log('Response', response);
```

### HTTP Request Update

Making an update call to a canister's `http_request_update` method:

```typescript
import { HttpAgent } from '@dfinity/agent';
import { HttpCanisterClient } from '@dfinity/http-canister-client';

const canisterId = 'qoctq-giaaa-aaaaa-aaaea-cai';

const agent = new HttpAgent();
const client = new HttpCanisterClient(canisterId, agent);

const response = await client.httpRequestUpdate({
  url: '/',
  method: 'GET',
  body: [],
  headers: [['accept-encoding', 'gzip']],
});

console.log('Response', response);
```
