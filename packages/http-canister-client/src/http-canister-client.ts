import { Principal } from '@dfinity/principal';
import { HttpAgent, ActorSubclass, Actor } from '@dfinity/agent';
import { idlFactory } from './http-interface/http-interface';
import {
  HttpRequest,
  HttpResponse,
  HttpUpdateRequest,
  _SERVICE,
} from './http-interface/http-interface-types';

export class HttpCanisterClient {
  private readonly actor: ActorSubclass<_SERVICE>;

  constructor(canisterId: string | Principal, agent: HttpAgent) {
    this.actor = Actor.createActor<_SERVICE>(idlFactory, {
      agent,
      canisterId,
    });
  }

  public async httpRequest(payload: HttpRequest): Promise<HttpResponse> {
    return await this.actor.http_request(payload);
  }

  public async httpRequestUpdate(
    payload: HttpUpdateRequest
  ): Promise<HttpResponse> {
    return await this.actor.http_request_update(payload);
  }
}
