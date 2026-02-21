import {
  HeaderField,
  HttpRequest,
  HttpResponse,
} from '@dfinity/http-canister-client';

import { getMaxResponseVerificationVersion } from './response-verification';

export async function mapRequest(req: Request): Promise<HttpRequest> {
  return {
    url: getRequestUrl(req),
    method: req.method,
    body: await getRequestBody(req),
    headers: getRequestHeaders(req),
    certificateVersion: getMaxResponseVerificationVersion(),
  };
}

function getRequestUrl(req: Request): string {
  const parsedUrl = new URL(req.url);

  return parsedUrl.pathname + parsedUrl.search + parsedUrl.hash;
}

async function getRequestBody(req: Request): Promise<Uint8Array> {
  const result = await req.arrayBuffer();

  return new Uint8Array(result);
}

function getRequestHeaders(req: Request): HeaderField[] {
  return Array.from(req.headers.entries());
}

export function mapResponse(res: HttpResponse): Response {
  return new Response(res.body, {
    headers: new Headers(res.headers),
    status: res.statusCode,
  });
}
