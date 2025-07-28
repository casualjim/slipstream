import { ApiException, fromHono } from "chanfana";
import { Hono } from "hono";
import { HTTPException } from "hono/http-exception";
import type { ContentfulStatusCode } from "hono/utils/http-status";
import { CreateAgent, DeleteAgent, GetAgent, ListAgents, UpdateAgent } from "./endpoints/agents";
import { GetModel, ListModels } from "./endpoints/models";
import { CreateOrganization, DeleteOrganization, GetOrganization, ListOrganizations, UpdateOrganization } from "./endpoints/organizations";
import { CreateProject, DeleteProject, GetProject, ListProjects, UpdateProject } from "./endpoints/projects";
import { CreateTool, DeleteTool, GetTool, ListTools, UpdateTool } from "./endpoints/tools";
// Import endpoints
import { bearerAuth } from "./middleware/auth";
import type { AppHonoEnv } from "./types";

const app = new Hono<AppHonoEnv>();

app.onError((err, c) => {
  if (err instanceof ApiException) {
    // If it's a Chanfana ApiException, let Chanfana handle the response
    return c.json({ success: false, errors: err.buildResponse() }, err.status as ContentfulStatusCode);
  }

  if (err instanceof HTTPException) {
    // If it's an HTTPException, return it directly
    return c.json({ success: false, errors: [{ code: err.status, message: err.message }] }, err.status);
  }

  // For other errors, return a generic 500 response
  return c.json(
    {
      success: false,
      errors: [{ code: 7000, message: "Internal Server Error" }],
    },
    500,
  );
});

// Apply auth middleware to all API routes
app.use("/api/v1/*", bearerAuth);

// Setup OpenAPI
const openapi = fromHono(app, {
  base: "/api/v1",
  docs_url: "/apidocs",
  schema: {
    info: {
      title: "Slipstream API",
      version: "1.0.0",
      description: "API for managing Slipstream agents, projects, organizations, and more.",
    },
  },
});

// Register endpoints
// Organizations
openapi.post("/organizations", CreateOrganization);
openapi.get("/organizations", ListOrganizations);
openapi.get("/organizations/:slug", GetOrganization);
openapi.put("/organizations/:slug", UpdateOrganization);
openapi.delete("/organizations/:slug", DeleteOrganization);

// Projects
openapi.post("/projects", CreateProject);
openapi.get("/projects", ListProjects);
openapi.get("/projects/:slug", GetProject);
openapi.put("/projects/:slug", UpdateProject);
openapi.delete("/projects/:slug", DeleteProject);

// // Tools
openapi.post("/tools", CreateTool);
openapi.get("/tools", ListTools);
openapi.get("/tools/:provider/:slug/:version", GetTool);
openapi.put("/tools/:provider/:slug/:version", UpdateTool);
openapi.delete("/tools/:provider/:slug/:version", DeleteTool);

// Models (read-only)
openapi.get("/models", ListModels);
openapi.get("/models/:id{.+}", GetModel);

// Agents
openapi.post("/agents", CreateAgent);
openapi.get("/agents", ListAgents);
openapi.get("/agents/:slug/:version", GetAgent);
openapi.put("/agents/:slug/:version", UpdateAgent);
openapi.delete("/agents/:slug/:version", DeleteAgent);

// Mount API
app.route("/api/v1", openapi);

// Health check
app.get("/health", (c) => c.json({ status: "ok" }));

// Catch-all handler for 404s to ensure JSON responses
app.notFound((c) => {
  return c.json(
    {
      success: false,
      errors: [{ code: 404, message: "Not Found" }],
    },
    404,
  );
});

// Export the Hono app
export default app;
