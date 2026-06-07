// @stats-code/server — Fastify router, handlers, middleware, CORS, load
// shedding, request-id. Maps to the Rust `agent-server` crate.
// Depends on: @stats-code/engine.

export interface HttpServer {
  start(opts: { host: '127.0.0.1'; port: number }): Promise<{ url: string }>;
  stop(): Promise<void>;
}
