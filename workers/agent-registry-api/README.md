# Slipstream Agent Registry API (Cloudflare Workers)

This Worker implements a REST API using Hono and @hono/zod-openapi. We compose routes in `src/hono/*`, expose a Hono app from `src/index.ts`, and serve an OpenAPI 3.1 document at `/openapi.json` with dynamic server URLs.

Key points:
- Framework: Hono (`OpenAPIHono`) with zod-based schema and OpenAPI generation
- Auth: minimal Bearer auth middleware for `/api/v1/**`
- Health: `GET /_health`
- API base: `/api/v1` with routers for tools, models, agents, organizations, and projects

## Get started

1. Install deps: `bun install`
2. Dev server: `bun run dev` (or `wrangler dev`)
3. OpenAPI spec: `GET /openapi.json` or `/api/v1/openapi.json`
4. Typegen + typecheck: `bun run typecheck`
5. Tests: `bun run test`

## Project structure

- Entry point: `src/index.ts` (exports the Hono app as default)
- App setup: `src/hono/index.ts` (OpenAPIHono, auth, routers, docs)
- Routers: `src/routes/*.routes.ts`
- Middleware: `src/middleware/*`
- Schemas: `src/schemas/*`
- Services and libs: `src/services/*`, `src/lib/*`

## Development

- Local dev: `bun run dev` and visit http://localhost:8787/
- OpenAPI JSON: http://localhost:8787/openapi.json
- 404s return JSON envelopes under the API paths
