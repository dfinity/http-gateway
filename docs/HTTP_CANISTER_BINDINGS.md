# HTTP Canister Bindings

## Candid Interface

The Candid interface is located in `./package/http-canister-client/src/http-interface/http-interface.did`.

## JavaScript binding

Generate the binding:

```shell
didc bind ./packages/http-canister-client/src/http-interface/http-interface.did --target js > ./packages/http-canister-client/src/http-interface/http-interface.ts
```

Then move the `StreamingCallbackHttpResponse` variable outside of the `idlFactory` function, rename to `streamingCallbackHttpResponseType` and then export it.

```typescript
export const streamingCallbackHttpResponseType = // ...
```

then add the `import { IDL } from '@dfinity/candid';` import, move the `Token` variable outside of the `idlFactory` function, and set its value to be `IDL.Unknown`.

```typescript
import { IDL } from '@dfinity/candid';

const Token = IDL.Unknown;
```

then add the type `IDL.InterfaceFactory` to the idlFactory export.

```typescript
export const idlFactory: IDL.InterfaceFactory = // ...
```

and finally, remove the unused init method `export const init`.

## TypeScript binding

Generate the binding:

```shell
didc bind ./packages/http-canister-client/src/http-interface/http-interface.did --target ts > ./packages/http-canister-client/src/http-interface/http-interface-types.d.ts
```

Add the following import:

```typescript
import { IDL } from '@dfinity/candid';
```

and then replace:

```typescript
export type Token = { type: any };
```

with:

```typescript
export type Token = { type: <T>() => IDL.Type<T> };
```
