import { HttpGatewayClient } from '@dfinity/http-gateway';
import { HttpAgent } from '@dfinity/agent';
import RESPONSE_VERIFICATION_WASM from '@dfinity/response-verification/dist/web/web_bg.wasm';

export default {
  async fetch(request, env): Promise<Response> {
    const cachedResponse = await caches.default.match(request);
    if (cachedResponse) {
      return mapResponse(cachedResponse);
    }

    const agent = HttpAgent.createSync({ host: env['API_GATEWAY'] });
    const client = new HttpGatewayClient({
      agent,
      responseVerificationWasm: RESPONSE_VERIFICATION_WASM,
      canisterId: env['CANISTER_ID'],
    });

    const response = await client.request({
      request,
    });

    await caches.default.put(request, mapResponse(response.clone()));

    return mapResponse(response);
  },
} satisfies ExportedHandler<Env>;

function mapResponse(res: Response): Response {
  return new Response(res.body, {
    headers: res.headers,
    status: res.status,
    // This prevents Cloudflare from automatically encoding the response body
    // according to the response's Content-Encoding header. Otherwise, the
    // response body would be double-encoded.
    //
    // See: https://developers.cloudflare.com/workers/runtime-apis/response/#parameters
    encodeBody: 'manual',
  });
}
