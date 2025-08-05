import { createRoute, OpenAPIHono, z as zOpenApi } from "@hono/zod-openapi";
import { z } from "zod";
import { ModelService, ProjectService, ToolService } from "../lib/services";
import { Agent, AgentSchema, CreateAgentSchema, UpdateAgentSchema } from "../schemas/agent";
import {
  ErrorEnvelopeOpenApi,
  failure,
  success,
  SuccessEnvelopeOpenApi,
  SuccessListEnvelopeOpenApi,
} from "../schemas/envelope";
import { AgentService } from "../services/agent.service";
import type { AppHonoEnv } from "../types";

// Shared param schemas
const SlugParam = z.string().min(1);
const VersionParam = z
  .string()
  .regex(
    /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/,
    "Invalid semantic version",
  );

// Factory
export function createAgentsRouter() {
  const app = new OpenAPIHono<AppHonoEnv>();

  // POST /
  const postAgent = createRoute({
    method: "post",
    path: "/",
    request: {
      body: {
        description: "Create a new Agent",
        content: {
          "application/json": {
            schema: zOpenApi.object({
              ...CreateAgentSchema.shape,
            }),
          },
        },
        required: true,
      },
    },
    responses: {
      201: {
        description: "Agent created",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(AgentSchema),
          },
        },
      },
      400: {
        description: "Validation error",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      401: {
        description: "Unauthorized",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      500: {
        description: "Server error",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
    },
    tags: ["agents"],
    summary: "Create Agent",
    description: "Creates a new agent in the registry",
  } as const);

  app.openapi(postAgent, async (c) => {
    try {
      const auth = c.get("auth");
      const body = await c.req.json();
      const input = await CreateAgentSchema.parseAsync(body);

      // Validate access to organization
      if (!auth.organizations.includes(input.organization)) {
        return c.json(failure("Access denied to organization"), 403);
      }

      // Validate project belongs to organization
      const projectService = new ProjectService(c.env.DB);
      if (!(await projectService.belongsToOrganization(input.project, input.organization))) {
        return c.json(failure("Project does not belong to organization"), 400);
      }

      // Validate model exists
      const modelService = new ModelService(c.env.DB);
      if (!(await modelService.exists(input.model))) {
        return c.json(failure("Invalid model ID"), 400);
      }

      // Validate tools if provided
      if (input.availableTools?.length) {
        const toolService = new ToolService(c.env.DB);
        if (!(await toolService.validateIds(input.availableTools))) {
          return c.json(failure("Invalid tool IDs"), 400);
        }
      }

      const service = new AgentService(c.env.DB);
      const created: Agent = await service.create(input, auth.userId);

      return c.json(success(created), 201);
    } catch (err: any) {
      if (err && typeof err === "object" && "issues" in (err as any)) {
        const first = (err as any).issues?.[0];
        return c.json(failure(first?.message ?? "Invalid input", undefined, first?.path?.join?.(".")), 400);
      }
      if (typeof err?.message === "string" && err.message.includes("UNIQUE")) {
        return c.json(failure("Conflict: agent already exists"), 409);
      }
      return c.json(failure(err?.message ?? "Unable to create agent"), 500);
    }
  });

  // GET /{slug}/{version}
  const getAgent = createRoute({
    method: "get",
    path: "/{slug}/{version}",
    request: {
      params: zOpenApi.object({
        slug: SlugParam,
        version: VersionParam,
      }),
    },
    responses: {
      200: {
        description: "Agent found",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(AgentSchema),
          },
        },
      },
      401: {
        description: "Unauthorized",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      404: {
        description: "Agent not found",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      500: {
        description: "Server error",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
    },
    tags: ["agents"],
    summary: "Get Agent",
    description: "Retrieve a specific agent by slug and version",
  } as const);

  app.openapi(getAgent, async (c) => {
    try {
      const { slug, version } = c.req.valid("param");
      const service = new AgentService(c.env.DB);
      const found = await service.get(slug, version);
      if (!found) return c.json(failure("Agent not found"), 404);
      return c.json(success(found));
    } catch (e: any) {
      return c.json(failure(e?.message ?? "Unable to get agent"), 500);
    }
  });

  // PUT /{slug}/{version}
  const putAgent = createRoute({
    method: "put",
    path: "/{slug}/{version}",
    request: {
      params: zOpenApi.object({
        slug: SlugParam,
        version: VersionParam,
      }),
      body: {
        description: "Partial Agent update",
        content: {
          "application/json": {
            schema: zOpenApi.object({
              ...UpdateAgentSchema.shape,
            }),
          },
        },
        required: true,
      },
    },
    responses: {
      200: {
        description: "Agent updated",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(AgentSchema),
          },
        },
      },
      400: {
        description: "Validation error",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      401: {
        description: "Unauthorized",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      404: {
        description: "Agent not found",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      500: {
        description: "Server error",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
    },
    tags: ["agents"],
    summary: "Update Agent",
    description: "Update an existing agent",
  } as const);

  app.openapi(putAgent, async (c) => {
    try {
      const auth = c.get("auth");
      const { slug, version } = c.req.valid("param");
      const body = await c.req.json();
      const patch = await UpdateAgentSchema.parseAsync(body);

      const service = new AgentService(c.env.DB);

      // First, get the existing agent to check access and validate updates
      const existing = await service.get(slug, version);
      if (!existing) {
        return c.json(failure("Not Found"), 404);
      }

      // Validate access to organization
      if (!auth.organizations.includes(existing.organization)) {
        return c.json(failure("Access denied to organization"), 403);
      }

      // Validate model if being updated
      if (patch.model) {
        const modelService = new ModelService(c.env.DB);
        if (!(await modelService.exists(patch.model))) {
          return c.json(failure("Invalid model ID"), 400);
        }
      }

      // Validate tools if being updated
      if (patch.availableTools?.length) {
        const toolService = new ToolService(c.env.DB);
        if (!(await toolService.validateIds(patch.availableTools))) {
          return c.json(failure("Invalid tool IDs"), 400);
        }
      }

      const updated = await service.update(slug, version, patch, auth.userId);
      if (!updated) return c.json(failure("Agent not found"), 404);
      return c.json({ success: true, result: updated });
    } catch (err: any) {
      if (err && typeof err === "object" && "issues" in (err as any)) {
        const first = (err as any).issues?.[0];
        return c.json(failure(first?.message ?? "Invalid input", undefined, first?.path?.join?.(".")), 400);
      }
      return c.json(failure(err?.message ?? "Unable to update agent"), 500);
    }
  });

  // DELETE /{slug}/{version}
  const deleteAgent = createRoute({
    method: "delete",
    path: "/{slug}/{version}",
    request: {
      params: zOpenApi.object({
        slug: SlugParam,
        version: VersionParam,
      }),
    },
    responses: {
      200: {
        description: "Agent deleted",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(zOpenApi.boolean()),
          },
        },
      },
      401: {
        description: "Unauthorized",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      404: {
        description: "Agent not found",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      500: {
        description: "Server error",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
    },
    tags: ["agents"],
    summary: "Delete Agent",
    description: "Delete a specific agent",
  } as const);

  app.openapi(deleteAgent, async (c) => {
    try {
      const auth = c.get("auth");
      const { slug, version } = c.req.valid("param");
      const service = new AgentService(c.env.DB);

      // First, get the existing agent to check access
      const existing = await service.get(slug, version);
      if (!existing) {
        return c.json(failure("Not Found"), 404);
      }

      // Validate access to organization
      if (!auth.organizations.includes(existing.organization)) {
        return c.json(failure("Access denied to organization"), 403);
      }

      const deleted = await service.delete(slug, version);
      if (!deleted) return c.json(failure("Not Found"), 404);
      return c.json({ success: true, result: true });
    } catch (e: any) {
      return c.json(failure(e?.message ?? "Unable to delete agent"), 500);
    }
  });

  // GET / (list)
  const listAgents = createRoute({
    method: "get",
    path: "/",
    request: {
      query: zOpenApi.object({
        page: zOpenApi.coerce.number().int().positive().optional(),
        per_page: zOpenApi.coerce.number().int().positive().max(100).optional(),
        search: zOpenApi.string().optional(),
        name: zOpenApi.string().optional(),
        orderBy: zOpenApi.enum(["name", "createdAt", "updatedAt"]).optional(),
        organization: zOpenApi.string().optional(),
        project: zOpenApi.string().optional(),
      }),
    },
    responses: {
      200: {
        description: "Agents list",
        content: {
          "application/json": {
            schema: SuccessListEnvelopeOpenApi(zOpenApi.array(AgentSchema)),
          },
        },
      },
      401: {
        description: "Unauthorized",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
    },
    tags: ["agents"],
    summary: "List Agents",
    description: "List agents with optional filters",
  } as const);

  app.openapi(listAgents, async (c) => {
    const q = c.req.valid("query");
    const service = new AgentService(c.env.DB);
    const res = await service.list({
      page: q.page,
      per_page: q.per_page,
      search: q.search,
      name: q.name,
      orderBy: q.orderBy,
      organization: q.organization,
      project: q.project,
    });
    return c.json({
      success: true,
      result: res.items,
      result_info: res.result_info,
    });
  });

  // GET /{slug} — latest by semantic version (stable preferred)
  const latestAgent = createRoute({
    method: "get",
    path: "/{slug}",
    request: {
      params: zOpenApi.object({
        slug: SlugParam,
      }),
    },
    responses: {
      200: {
        description: "Latest Agent",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(AgentSchema),
          },
        },
      },
      401: {
        description: "Unauthorized",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      404: {
        description: "Agent not found",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      500: {
        description: "Server error",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
    },
    tags: ["agents"],
    summary: "Get Latest Agent",
    description: "Retrieve the latest version of an agent for a slug",
  } as const);

  app.openapi(latestAgent, async (c) => {
    try {
      const auth = c.get("auth");
      const { slug } = c.req.valid("param");
      const service = new AgentService(c.env.DB);
      const item = await service.getLatestBySlug(slug);
      if (!item) return c.json(failure("Agent not found"), 404);

      // Validate access to organization
      if (!auth.organizations.includes(item.organization)) {
        return c.json(failure("Agent not found"), 404);
      }

      return c.json(success(item));
    } catch (e: any) {
      return c.json(failure(e?.message ?? "Unable to get latest agent"), 500);
    }
  });

  return app;
}
