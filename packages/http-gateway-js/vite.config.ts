import { resolve } from 'path';
import { defineConfig } from 'vitest/config';
import checker from 'vite-plugin-checker';

export default defineConfig({
  plugins: [checker({ typescript: true })],
  build: {
    lib: {
      entry: resolve(__dirname, 'src', 'index.ts'),
      name: '@dfinity/http-gateway',
      fileName: 'http-gateway',
    },
    sourcemap: true,
    rollupOptions: {
      external: [
        '@dfinity/response-verification',
        '@dfinity/http-canister-client',
        '@dfinity/agent',
        '@dfinity/principal',
        '@dfinity/candid',
      ],
      output: {
        globals: {
          '@dfinity/response-verification': 'dfinity-response-verification',
          '@dfinity/http-canister-client': 'dfinity-http-canister-client',
          '@dfinity/agent': 'dfinity-agent',
          '@dfinity/principal': 'dfinity-principal',
          '@dfinity/candid': 'dfinity-candid',
        },
      },
    },
  },
});
