/**
 * Cloudflare Worker entry – re-export the Hono app's fetch handler.
 * The Hono application is defined in workers/agent-registry-api/src/hono/index.ts.
 * We export a standard { fetch } shape compatible with Cloudflare Workers.
 */
import { createApp } from "./hono";

export const app = createApp();
export default app;
