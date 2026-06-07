// contract/json-schema.ts — generate JSON Schema from the zod route contracts
// for Fastify validation (task 3.1 → consumed by task 3.2).

import { zodToJsonSchema } from 'zod-to-json-schema';
import { ROUTE_CONTRACTS, type RouteContract } from './routes.js';

export interface RouteJsonSchema {
  id: string;
  method: string;
  path: string;
  successStatus: number;
  bodyLimitBytes?: number;
  body?: unknown;
  response?: unknown;
}

/** Convert a single route contract's zod schemas to JSON Schema. */
export function routeToJsonSchema(route: RouteContract): RouteJsonSchema {
  return {
    id: route.id,
    method: route.method,
    path: route.path,
    successStatus: route.successStatus,
    bodyLimitBytes: route.bodyLimitBytes,
    body: route.request
      ? zodToJsonSchema(route.request, { target: 'jsonSchema7', $refStrategy: 'none' })
      : undefined,
    response: route.response
      ? zodToJsonSchema(route.response, { target: 'jsonSchema7', $refStrategy: 'none' })
      : undefined,
  };
}

/** All route JSON Schemas, keyed by route id. */
export function allRouteJsonSchemas(): Record<string, RouteJsonSchema> {
  const out: Record<string, RouteJsonSchema> = {};
  for (const route of ROUTE_CONTRACTS) {
    out[route.id] = routeToJsonSchema(route);
  }
  return out;
}
