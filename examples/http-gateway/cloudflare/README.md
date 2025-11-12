# Cloudflare Worker based ICP HTTP Gateway

## Environment variables

Set the `API_GATEWAY` and `CANISTER_ID` variables in the `wrangler.json` file. Run `pnpm run -F http-gateway-cloudflare-example cf-typegen` to regenerate the `worker-configuration.d.ts` file.

## Notes

- Do not alter the response body, status codes or headers in any way. Local HTTP Gateways on an end user's computer may re-validate the response and reject it if it has been altered.

## Possible optimizations

- Early hints:
  - https://developers.cloudflare.com/workers/examples/103-early-hints/.
  - https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Link.
  - Link headers should be set on the canister side and certified. The implementation of this will depend on the frontend framework and bundler that is used.
- Cache control:
  - Cloudflare reads the `Cache-Control` header and caches the response based on the value of the header.
  - This header should be completely controlled by the canister.
