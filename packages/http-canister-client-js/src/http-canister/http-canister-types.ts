import { Principal } from '@dfinity/principal';
import { IDL } from '@dfinity/candid';

export type HeaderField = [string, string];

export interface HttpRequest {
  url: string;
  method: string;
  body: Uint8Array;
  headers: HeaderField[];
  certificateVersion?: number | null;
}

export interface HttpResponse {
  body: Uint8Array;
  headers: Array<HeaderField>;
  upgrade: boolean | null;
  streamingStrategy: StreamingStrategy | null;
  statusCode: number;
}

export interface HttpUpdateRequest {
  url: string;
  method: string;
  body: Uint8Array;
  headers: HeaderField[];
}

export interface StreamingCallbackHttpResponse {
  token: Token | null;
  body: Uint8Array;
}

export type StreamingStrategy = {
  Callback: { token: Token; callback: [Principal, string] };
};

export type Token = { type: <T>() => IDL.Type<T> };
