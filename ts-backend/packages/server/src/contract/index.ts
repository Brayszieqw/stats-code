// contract/index.ts — public surface of the API contract harness.

export * as domain from './domain.js';
export * as sidecar from './sidecar.js';
export {
  ROUTE_CONTRACTS,
  type RouteContract,
  type HttpMethod,
  healthResponse,
  patchSettingsRequest,
  patchResearchProtocolRequest,
  protocolCompileRequest,
  protocolCompileResult,
  base64DatasetRequest,
  postAudioResponse,
  llmStatusResponse,
  postLlmConfigRequest,
  postLlmConfigActivateRequest,
  postMessageRequest,
} from './routes.js';
export { allRouteJsonSchemas, routeToJsonSchema, type RouteJsonSchema } from './json-schema.js';
export { HTTP_STATUS_FOR } from './domain.js';
