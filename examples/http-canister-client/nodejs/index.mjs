import fetch from 'isomorphic-fetch';
import { HttpAgent } from '@dfinity/agent';
import { HttpCanisterClient } from '@dfinity/http-canister-client';

const canisterId = 'qoctq-giaaa-aaaaa-aaaea-cai';

const agent = new HttpAgent({ fetch, host: 'https://icp-api.io' });
const client = new HttpCanisterClient(canisterId, agent);

const response = await client.httpRequest({
  url: '/',
  method: 'GET',
  body: [],
  headers: [['accept-encoding', 'gzip']],
  certificate_version: [],
});

console.log('Response', response);
