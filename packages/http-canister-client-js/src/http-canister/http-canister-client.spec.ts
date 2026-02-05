import { Actor, ActorSubclass } from '@dfinity/agent';
import { describe, it, expect, vi, beforeEach, Mock, Mocked } from 'vitest';

import {
  _SERVICE,
  HttpResponse as CanisterHttpResponse,
  HttpRequest as CanisterHttpRequest,
  idlFactory,
} from '../http-interface';
import { HttpCanisterClient } from './http-canister-client';
import { Principal } from '@dfinity/principal';
import { HttpRequest, HttpResponse } from './http-canister-types';

const CANISTER_ID = Principal.fromUint8Array(new Uint8Array([0]));
type MockedActor = Mocked<ActorSubclass<_SERVICE>>;

describe('httpCanisterClient', () => {
  let agentMock: Mock;
  let actorMock: Omit<MockedActor, 'metadataSymbol'>;

  let client: HttpCanisterClient;

  beforeEach(() => {
    agentMock = vi.fn();

    actorMock = {
      http_request: Object.assign(vi.fn(), { withOptions: vi.fn() }),
      http_request_update: Object.assign(vi.fn(), { withOptions: vi.fn() }),
    };

    vi.spyOn(Actor, 'createActor').mockReturnValue(actorMock as MockedActor);

    client = new HttpCanisterClient({
      agent: agentMock as any,
      canisterId: CANISTER_ID,
    });
  });

  it('should create', () => {
    expect(client).toBeDefined();
    expect(client).toBeInstanceOf(HttpCanisterClient);
    expect(Actor.createActor).toHaveBeenCalledWith(idlFactory, {
      agent: agentMock,
      canisterId: CANISTER_ID,
    });
  });

  describe('httpRequest()', () => {
    it('should send an HTTP request', async () => {
      const canisterReq: CanisterHttpRequest = {
        method: 'GET',
        url: '/index.html',
        body: new Uint8Array(),
        headers: [],
        certificate_version: [],
      };
      const canisterRes: CanisterHttpResponse = {
        status_code: 200,
        body: new Uint8Array([0, 1, 2, 3, 4, 5]),
        headers: [],
        streaming_strategy: [],
        upgrade: [],
      };

      actorMock.http_request.mockResolvedValue(canisterRes);

      const req: HttpRequest = {
        method: 'GET',
        url: '/index.html',
        body: new Uint8Array(),
        headers: [],
        certificateVersion: null,
      };
      const expectedRes: HttpResponse = {
        statusCode: 200,
        body: new Uint8Array([0, 1, 2, 3, 4, 5]),
        headers: [],
        streamingStrategy: null,
        upgrade: null,
      };
      const res = await client.httpRequest(req);

      expect(actorMock.http_request).toHaveBeenCalledWith(canisterReq);
      expect(res).toEqual(expectedRes);
    });
  });

  describe('httpRequestUpdate()', () => {
    it('should send an HTTP request update', async () => {
      const canisterReq: CanisterHttpRequest = {
        method: 'POST',
        url: '/update',
        body: new Uint8Array([1, 2, 3]),
        headers: [],
        certificate_version: [],
      };
      const canisterRes: CanisterHttpResponse = {
        status_code: 200,
        body: new Uint8Array([0, 1, 2, 3, 4, 5]),
        headers: [],
        streaming_strategy: [],
        upgrade: [],
      };

      actorMock.http_request_update.mockResolvedValue(canisterRes);

      const req: HttpRequest = {
        method: 'POST',
        url: '/update',
        body: new Uint8Array([1, 2, 3]),
        headers: [],
        certificateVersion: null,
      };
      const expectedRes: HttpResponse = {
        statusCode: 200,
        body: new Uint8Array([0, 1, 2, 3, 4, 5]),
        headers: [],
        streamingStrategy: null,
        upgrade: null,
      };
      const res = await client.httpRequestUpdate(req);

      expect(actorMock.http_request_update).toHaveBeenCalledWith(canisterReq);
      expect(res).toEqual(expectedRes);
    });
  });
});
