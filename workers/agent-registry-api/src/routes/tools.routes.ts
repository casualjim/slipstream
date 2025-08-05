import { createRoute, OpenAPIHono, z as zOpenApi } from "@hono/zod-openapi";
import { z } from "zod";
import {
  ErrorEnvelopeOpenApi,
  failure,
  SuccessEnvelopeOpenApi,
  SuccessListEnvelopeOpenApi
} from "../schemas/envelope";
import { CreateToolSchema, ToolSchema, UpdateToolSchema } from "../schemas/tool";
import { ToolService } from "../services/tool.service";
import type { AppHonoEnv } from "../types";

// Path param schemas
const ProviderParam = z.enum(["Client", "Local", "MCP", "Restate"]);
const SlugParam = z.string().min(1);
const VersionParam = z
  .string()
  .regex(
    /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/,
    "Invalid semantic version",
  );

// Factory
export function createToolsRouter() {
  const app = new OpenAPIHono<AppHonoEnv>();

  // OpenAPI route definitions

  const postTool = createRoute({
    method: "post",
    path: "/",
    request: {
      body: {
        description: "Create a new Tool",
        content: {
          "application/json": {
            schema: zOpenApi.object({
              ...CreateToolSchema.shape,
            }),
          },
        },
        required: true,
      },
    },
    responses: {
      201: {
        description: "Tool created",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(ToolSchema),
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
      }
    },
    tags: ["tools"],
    summary: "Create Tool",
    description: "Creates a new tool in the registry",
  } as const);

  app.openapi(postTool, async (c) => {
    try {
      const body = await c.req.json();
      const parsed = await CreateToolSchema.parseAsync(body);
      const service = new ToolService(c.env.DB);
      const created = await service.create(parsed);
      // Explicitly annotate envelope to satisfy app.openapi typing
      const envelope: { success: true; result: z.infer<typeof ToolSchema> } = {
        success: true,
        result: created,
      };
      return c.json(envelope, 201);
    } catch (err) {
      if (err && typeof err === "object" && "issues" in (err as any)) {
        const first = (err as any).issues?.[0];
        const badReq = failure(first?.message ?? "Invalid input", undefined, first?.path?.join?.("."));
        return c.json(badReq, 400);
      }
      const serverErr = failure("Unable to create tool");
      return c.json(serverErr, 500);
    }
  });

  const getTool = createRoute({
    method: "get",
    path: "/{provider}/{slug}/{version}",
    request: {
      params: zOpenApi.object({
        provider: ProviderParam,
        slug: SlugParam,
        version: VersionParam,
      }),
    },
    responses: {
      200: {
        description: "Tool found",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(ToolSchema),
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
        description: "Tool not found",
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
    tags: ["tools"],
    summary: "Get Tool",
    description: "Retrieve a specific tool by provider, slug, and version",
  });

  app.openapi(getTool, async (c) => {
    try {
      const { provider, slug, version } = c.req.valid("param");
      const service = new ToolService(c.env.DB);
      const found = await service.get(provider, slug, version);
      if (!found) {
        return c.json(failure("Not Found"), 404);
      }
      return c.json({ success: true, result: found });
    } catch (e: any) {
      return c.json(failure(e?.message ?? "Unable to get tool"), 500);
    }
  });

  const putTool = createRoute({
    method: "put",
    path: "/{provider}/{slug}/{version}",
    request: {
      params: zOpenApi.object({
        provider: ProviderParam,
        slug: SlugParam,
        version: VersionParam,
      }),
      body: {
        description: "Partial Tool update",
        content: {
          "application/json": {
            schema: zOpenApi.object({
              ...UpdateToolSchema.shape,
            }),
          },
        },
        required: true,
      },
    },
    responses: {
      200: {
        description: "Tool updated",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(ToolSchema),
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
        description: "Tool not found",
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
    } as const,
    tags: ["tools"],
    summary: "Update Tool",
    description: "Update an existing tool",
  });

  app.openapi(putTool, async (c) => {
    try {
      const { provider, slug, version } = c.req.valid("param");
      const body = await c.req.json();
      const patch = await UpdateToolSchema.parseAsync(body);
      const service = new ToolService(c.env.DB);
      const updated = await service.update(provider, slug, version, patch);
      if (!updated) {
        return c.json(failure("Not Found"), 404);
      }
      return c.json({ success: true, result: updated });
    } catch (err) {
      if (err && typeof err === "object" && "issues" in (err as any)) {
        const first = (err as any).issues?.[0];
        return c.json(failure(first?.message ?? "Invalid input", undefined, first?.path?.join?.(".")), 400);
      }
      return c.json(failure("Unable to update tool"), 500);
    }
  });

  const deleteTool = createRoute({
    method: "delete",
    path: "/{provider}/{slug}/{version}",
    request: {
      params: zOpenApi.object({
        provider: ProviderParam,
        slug: SlugParam,
        version: VersionParam,
      }),
    },
    responses: {
      200: {
        description: "Tool deleted",
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
        description: "Tool not found",
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
    } as const,
    tags: ["tools"],
    summary: "Delete Tool",
    description: "Delete a specific tool",
  });

  app.openapi(deleteTool, async (c) => {
    try {
      const { provider, slug, version } = c.req.valid("param");
      const service = new ToolService(c.env.DB);
      const deleted = await service.delete(provider, slug, version);
      if (!deleted) {
        return c.json(failure("Not Found"), 404);
      }
      return c.json({ success: true, result: true });
    } catch (e: any) {
      return c.json(failure(e?.message ?? "Unable to delete tool"), 500);
    }
  });

  const listTools = createRoute({
    method: "get",
    path: "/",
    request: {
      query: zOpenApi.object({
        page: zOpenApi.coerce.number().int().positive().optional(),
        per_page: zOpenApi.coerce.number().int().positive().max(100).optional(),
        search: zOpenApi.string().optional(),
        name: zOpenApi.string().optional(),
        provider: ProviderParam.optional(),
        orderBy: zOpenApi.enum(["name", "createdAt", "updatedAt"]).optional(),
      }),
    },
    responses: {
      200: {
        description: "Tools list",
        content: {
          "application/json": {
            schema: SuccessListEnvelopeOpenApi(zOpenApi.array(ToolSchema)),
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
    tags: ["tools"],
    summary: "List Tools",
    description: "List all tools",
  });

  app.openapi(listTools, async (c) => {
    const q = c.req.valid("query");
    const service = new ToolService(c.env.DB);
    const res = await service.list({
      page: q.page,
      per_page: q.per_page,
      search: q.search,
      name: q.name,
      provider: q.provider,
      orderBy: q.orderBy,
    });
    return c.json({
      success: true,
      result: res.items,
      result_info: res.result_info,
    });
  });

  // New primary latest endpoint matching tests: GET /{provider}/{slug}
  const latestTool = createRoute({
    method: "get",
    path: "/{provider}/{slug}",
    request: {
      params: zOpenApi.object({
        provider: ProviderParam,
        slug: SlugParam,
      }),
    },
    responses: {
      200: {
        description: "Latest Tool",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(ToolSchema),
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
        description: "Tool not found",
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
    tags: ["tools"],
    summary: "Get Latest Tool",
    description: "Retrieve the latest version of a tool for a provider/slug",
  });

  app.openapi(latestTool, async (c) => {
    try {
      const { provider, slug } = c.req.valid("param");
      const service = new ToolService(c.env.DB);
      const item = await service.getLatestBySlug(provider, slug);
      if (!item) {
        return c.json(failure("Not Found"), 404);
      }
      return c.json({ success: true, result: item });
    } catch (e: any) {
      return c.json(failure(e?.message ?? "Unable to get latest tool"), 500);
    }
  });

  return app;
}
