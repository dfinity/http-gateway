import { HttpCanisterClient } from '@dfinity/http-canister-client';
import { Principal } from '@dfinity/principal';

import { mapRequest, mapResponse } from './mappings';
import { responseVerification } from './response-verification';

export interface ProcessRequestArgs {
  request: Request;
  rootKey: Uint8Array;
  canisterId: Principal;
  canister: HttpCanisterClient;
}

export async function processRequest({
  request,
  rootKey,
  canisterId,
  canister,
}: ProcessRequestArgs): Promise<Response> {
  const canisterReq = await mapRequest(request);
  const queryRes = await canister.httpRequest(canisterReq);

  if (queryRes.upgrade) {
    const updateRes = await canister.httpRequestUpdate(canisterReq);

    return mapResponse(updateRes);
  }

  responseVerification({
    canisterId,
    rootKey,
    request: canisterReq,
    response: queryRes,
  });

  return mapResponse(queryRes);
}
