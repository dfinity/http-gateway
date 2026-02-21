import { HttpAgent } from '@dfinity/agent';
import { Principal } from '@dfinity/principal';
import initResponseVerification from '@dfinity/response-verification';
import { HttpCanisterClient } from '@dfinity/http-canister-client';

import { processRequest } from './protocol';

const DEFAULT_API_GATEWAY = 'https://icp-api.io';

export interface HttpGatewayClientArgs {
  agent?: HttpAgent | null;
  responseVerificationWasm?: Uint8Array;
  canisterId: string | Principal;
}

export interface HttpGatewayClientRequestArgs {
  request: Request;
}

export class HttpGatewayClient {
  readonly #responseVerificationWasm?: Uint8Array;
  readonly #rootKey: Uint8Array;
  readonly #canister: HttpCanisterClient;
  readonly #canisterId: Principal;

  constructor({
    agent,
    responseVerificationWasm,
    canisterId,
  }: HttpGatewayClientArgs) {
    agent = agent ?? HttpAgent.createSync({ host: DEFAULT_API_GATEWAY });

    this.#canisterId = Principal.from(canisterId);
    this.#rootKey = new Uint8Array(agent.rootKey);
    this.#canister = new HttpCanisterClient({
      agent,
      canisterId: this.#canisterId,
    });
    this.#responseVerificationWasm = responseVerificationWasm;
  }

  public async request({
    request,
  }: HttpGatewayClientRequestArgs): Promise<Response> {
    if (this.#responseVerificationWasm) {
      await initResponseVerification(this.#responseVerificationWasm);
    }

    return await processRequest({
      request,
      canisterId: this.#canisterId,
      rootKey: this.#rootKey,
      canister: this.#canister,
    });
  }
}
