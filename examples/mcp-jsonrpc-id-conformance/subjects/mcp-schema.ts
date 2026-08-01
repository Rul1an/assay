export type RequestId = string | number;

export interface JSONRPCErrorResponse {
  jsonrpc: typeof JSONRPC_VERSION;
  id?: RequestId;
  error: Error;
}
