import { HttpRequest, HttpResponse } from '@dfinity/http-canister-client';
import { Principal } from '@dfinity/principal';
import {
  VerificationInfo,
  verifyRequestResponsePair,
  getMaxVerificationVersion,
} from '@dfinity/response-verification';

export const MIN_VERIFICATION_VERSION = 2;

export function getMaxResponseVerificationVersion(): number {
  return getMaxVerificationVersion();
}

export interface ResponseVerificationArgs {
  canisterId: Principal;
  rootKey: Uint8Array;
  request: HttpRequest;
  response: HttpResponse;
}

const NS_PER_MS = 1_000_000;
const NS_PER_SEC = 1_000_000_000;
const S_PER_MINS = 60;
const FIVE_MINS_NS = 5 * S_PER_MINS * NS_PER_SEC;

const MAX_CERT_OFFSET_NS = BigInt(FIVE_MINS_NS);

export async function responseVerification({
  canisterId,
  rootKey,
  request,
  response,
}: ResponseVerificationArgs): Promise<VerificationInfo> {
  const currentTimeMs = Date.now();
  const currentTimeNs = BigInt(currentTimeMs * NS_PER_MS);

  return verifyRequestResponsePair(
    {
      url: request.url,
      headers: request.headers,
      method: request.method,
      body: request.body,
      certificate_version: request.certificateVersion
        ? [request.certificateVersion]
        : [],
    },
    {
      status_code: response.statusCode,
      headers: response.headers,
      body: response.body,
    },
    canisterId.toUint8Array(),
    currentTimeNs,
    MAX_CERT_OFFSET_NS,
    new Uint8Array(rootKey),
    MIN_VERIFICATION_VERSION,
  );
}
