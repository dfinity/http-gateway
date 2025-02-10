import { ActorSubclass, Actor, HttpAgent } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';

import { idlFactory, _SERVICE } from '../http-interface';
import { mapHttpRequest, mapHttpResponse } from './http-canister-mappings';
import {
  HttpRequest,
  HttpUpdateRequest,
  HttpResponse,
} from './http-canister-types';

export interface HttpCanisterClientArgs {
  canisterId: string | Principal;
  agent: HttpAgent;
}

export class HttpCanisterClient {
  readonly #actor: ActorSubclass<_SERVICE>;

  constructor({ agent, canisterId }: HttpCanisterClientArgs) {
    this.#actor = Actor.createActor<_SERVICE>(idlFactory, {
      agent,
      canisterId,
    });
  }

  public async httpRequest(req: HttpRequest): Promise<HttpResponse> {
    const canisterReq = mapHttpRequest(req);
    const canisterRes = await this.#actor.http_request(canisterReq);
    return mapHttpResponse(canisterRes);
  }

  public async httpRequestUpdate(
    req: HttpUpdateRequest,
  ): Promise<HttpResponse> {
    const canisterReq = mapHttpRequest(req);
    const canisterRes = await this.#actor.http_request_update(canisterReq);
    return mapHttpResponse(canisterRes);
  }
}
