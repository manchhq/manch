import { createConnectTransport } from "@connectrpc/connect-web";

/** Default localhost endpoint of manch-server. */
export const DEFAULT_BASE_URL = "http://127.0.0.1:8787";

export function createManchTransport(baseUrl: string = DEFAULT_BASE_URL) {
  return createConnectTransport({ baseUrl });
}
