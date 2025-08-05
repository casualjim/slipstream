import { OpenAPIHono } from "@hono/zod-openapi";
import { bearerAuth } from "../middleware/auth";
import { onError } from "../middleware/error";
import { createAgentsRouter } from "../routes/agents.routes";
import { createModelsRouter } from "../routes/models.routes";
import { createOrganizationsRouter } from "../routes/organizations.routes";
import { createProjectsRouter } from "../routes/projects.routes";
import { createToolsRouter } from "../routes/tools.routes";
import type { AppHonoEnv, AuthContext } from "../types";

/**
 * Request context middleware
 * - Attaches a minimal auth context onto c.var.auth
 * - In a real system, this would parse headers/session to populate user/orgs
 */
function requestContextMiddleware() {
  return async (c: import("hono").Context<AppHonoEnv>, next: () => Promise<void>) => {
    const auth: AuthContext = {
      userId: "anonymous",
      organizations: [],
    };
    c.set("auth", auth);
    await next();
  };
}

/**
 * Create Hono app
 * - registers global error handler
 * - registers context middleware
 * - adds /_health route
 * - mounts versioned API routers at /api/v1
 * - wires hono-openapi so that app.openapi(...) is available on routers
 */
export function createApp() {
  // Use OpenAPIHono so `.openapi(...)` exists for routers like tools.routes.ts
  const app = new OpenAPIHono<AppHonoEnv>();

  // Register global error handler
  app.onError(onError);

  // Attach request context to each request
  app.use("*", requestContextMiddleware());

  // Health endpoint
  app.get("/_health", (c) => c.json({ status: "ok" }, 200));

  // Versioned API group — also OpenAPIHono to ensure nested routers can use `.openapi(...)`
  const api = new OpenAPIHono<AppHonoEnv>();

  // Apply Bearer auth to all API routes (tests expect 401 when missing/invalid Authorization)
  api.use("*", bearerAuth);

  // Mount routers at their base (routers will use relative paths)
  api.route("/tools", createToolsRouter());
  api.route("/models", createModelsRouter());
  api.route("/agents", createAgentsRouter());
  api.route("/organizations", createOrganizationsRouter());
  api.route("/projects", createProjectsRouter());

  // Mount at /api/v1
  app.route("/api/v1", api);

  // Serve OpenAPI JSON with dynamic server URLs
  app.doc("/openapi.json", (c) => {
    const url = new URL(c.req.url);
    const baseUrl = `${url.protocol}//${url.host}`;
    
    return {
      openapi: "3.1.0",
      info: {
        title: "Slipstream Agent Registry API",
        version: "0.1.0",
        description: "OpenAPI 3.1 spec for Slipstream Agent Registry composed via @hono/zod-openapi.",
      },
      servers: [
        { url: baseUrl, description: "Current environment" },
      ],
    };
  });
  app.get("/api/v1/openapi.json", (c) =>
    app.fetch(new Request(new URL("/openapi.json", c.req.url), { headers: c.req.raw.headers })),
  );

  // Ensure 404s under API return JSON envelope, not text
  app.notFound((c) =>
    c.json(
      {
        success: false,
        errors: [{ message: "Not Found" }],
      },
      404,
    ),
  );

  return app;
}

export type App = ReturnType<typeof createApp>;
