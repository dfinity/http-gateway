import { fromCandidOpt, toCandidOpt } from '../util';
import {
  HttpRequest as CanisterHttpRequest,
  HttpResponse as CanisterHttpResponse,
} from '../http-interface';
import { HttpRequest, HttpResponse } from './http-canister-types';

export function mapHttpRequest(req: HttpRequest): CanisterHttpRequest {
  return {
    url: req.url,
    method: req.method,
    body: req.body,
    headers: req.headers,
    certificate_version: toCandidOpt(req.certificateVersion),
  };
}

export function mapHttpResponse(res: CanisterHttpResponse): HttpResponse {
  return {
    body: new Uint8Array(res.body),
    headers: res.headers,
    upgrade: fromCandidOpt(res.upgrade),
    streamingStrategy: fromCandidOpt(res.streaming_strategy),
    statusCode: res.status_code,
  };
}
