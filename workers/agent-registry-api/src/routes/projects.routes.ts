import { createRoute, OpenAPIHono, z as zOpenApi } from "@hono/zod-openapi";
import { z } from "zod";
import { generateSlug, isValidSlug } from "../lib/utils";
import { ErrorEnvelopeOpenApi, failure, SuccessEnvelopeOpenApi, SuccessListEnvelopeOpenApi } from "../schemas/envelope";
import { CreateProjectSchema, ProjectService, UpdateProjectSchema } from "../services/project.service";
import type { AppHonoEnv } from "../types";
import { ProjectSchema } from "../types";

// Param schemas
const SlugParam = z.string().min(1);

// Factory
export function createProjectsRouter() {
  const app = new OpenAPIHono<AppHonoEnv>();

  // POST /
  const postProject = createRoute({
    method: "post",
    path: "/",
    request: {
      body: {
        description: "Create a new Project",
        content: {
          "application/json": {
            schema: zOpenApi.object({
              ...CreateProjectSchema.shape,
            }),
          },
        },
        required: true,
      },
    },
    responses: {
      201: {
        description: "Project created",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(ProjectSchema),
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
      403: {
        description: "Forbidden",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      409: {
        description: "Conflict - Duplicate slug",
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
    tags: ["projects"],
    summary: "Create Project",
    description: "Creates a new project in the registry",
  } as const);

  app.openapi(postProject, async (c) => {
    try {
      const body = await c.req.json();
      const input = await CreateProjectSchema.parseAsync(body);
      const auth = c.get("auth");
      const service = new ProjectService(c.env.DB);

      // Validate access to organization
      if (!auth.organizations.includes(input.organization)) {
        return c.json(failure("Access denied to organization"), 403);
      }

      const slug = input.slug ?? generateSlug(input.name);
      if (!isValidSlug(slug)) {
        return c.json(failure("Invalid slug format"), 400);
      }

      if (await service.checkSlugExists(slug)) {
        return c.json(failure("Slug already exists"), 400);
      }

      const created = await service.create({ ...input, slug });

      return c.json(
        {
          success: true,
          result: created,
        },
        201,
      );
    } catch (err) {
      if (err && typeof err === "object" && "issues" in err) {
        const first = (err as any).issues?.[0];
        return c.json(failure(first?.message ?? "Invalid input", undefined, first?.path?.join?.(".")), 400);
      }
      if (
        err &&
        typeof err === "object" &&
        "message" in err &&
        typeof err.message === "string" &&
        err.message.includes("UNIQUE")
      ) {
        return c.json(failure("Conflict: project already exists"), 409);
      }
      if (err && typeof err === "object" && "message" in err && typeof err.message === "string") {
        return c.json(failure(err.message, undefined, "error"), 500);
      }
      return c.json(failure("Unable to create project"), 500);
    }
  });

  // GET /{slug}
  const getProject = createRoute({
    method: "get",
    path: "/{slug}",
    request: {
      params: zOpenApi.object({
        slug: SlugParam,
      }),
    },
    responses: {
      200: {
        description: "Project found",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(ProjectSchema),
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
        description: "Project not found",
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
    tags: ["projects"],
    summary: "Get Project",
    description: "Retrieve a project by slug",
  } as const);

  app.openapi(getProject, async (c) => {
    try {
      const { slug } = c.req.valid("param");
      if (!isValidSlug(slug)) {
        return c.json(failure("Invalid slug format"), 400);
      }
      const service = new ProjectService(c.env.DB);
      const found = await service.get(slug);

      if (!found) {
        return c.json(failure("Not Found"), 404);
      }

      return c.json({
        success: true,
        result: found,
      });
    } catch (e: any) {
      return c.json(failure(e?.message ?? "Unable to get project"), 500);
    }
  });

  // PUT /{slug}
  const putProject = createRoute({
    method: "put",
    path: "/{slug}",
    request: {
      params: zOpenApi.object({
        slug: SlugParam,
      }),
      body: {
        description: "Partial Project update",
        content: {
          "application/json": {
            schema: zOpenApi.object({
              ...UpdateProjectSchema.shape,
            }),
          },
        },
        required: true,
      },
    },
    responses: {
      200: {
        description: "Project updated",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(ProjectSchema),
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
      403: {
        description: "Forbidden",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      404: {
        description: "Project not found",
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
    tags: ["projects"],
    summary: "Update Project",
    description: "Update an existing project",
  } as const);

  app.openapi(putProject, async (c) => {
    try {
      const { slug } = c.req.valid("param");
      const body = await c.req.json();
      const patch = await UpdateProjectSchema.parseAsync(body);
      const auth = c.get("auth");
      const service = new ProjectService(c.env.DB);

      const existing = await service.get(slug);
      if (!existing) {
        return c.json(failure("Not Found"), 404);
      }

      if (!auth.organizations.includes(existing.organization)) {
        return c.json(failure("Access denied to organization"), 403);
      }

      if (patch.organization && !auth.organizations.includes(patch.organization)) {
        return c.json(failure("Access denied to organization"), 403);
      }

      const updated = await service.update(slug, patch);
      if (!updated) {
        return c.json(failure("Unable to update project"), 500);
      }

      return c.json({
        success: true,
        result: updated,
      });
    } catch (err) {
      if (err && typeof err === "object" && "issues" in (err as any)) {
        const first = (err as any).issues?.[0];
        return c.json(failure(first?.message ?? "Invalid input", undefined, first?.path?.join?.(".")), 400);
      }
      if (err && typeof err === "object" && "message" in err && typeof err.message === "string") {
        return c.json(failure(err.message, undefined, "error"), 500);
      }
      return c.json(failure("Unable to update project"), 500);
    }
  });

  // DELETE /{slug}
  const deleteProject = createRoute({
    method: "delete",
    path: "/{slug}",
    request: {
      params: zOpenApi.object({
        slug: SlugParam,
      }),
    },
    responses: {
      200: {
        description: "Project deleted",
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
      403: {
        description: "Forbidden",
        content: {
          "application/json": {
            schema: ErrorEnvelopeOpenApi,
          },
        },
      },
      404: {
        description: "Project not found",
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
    tags: ["projects"],
    summary: "Delete Project",
    description: "Delete a specific project",
  } as const);

  app.openapi(deleteProject, async (c) => {
    try {
      const { slug } = c.req.valid("param");
      const auth = c.get("auth");
      const service = new ProjectService(c.env.DB);

      const existing = await service.get(slug);
      if (!existing) {
        return c.json(failure("Not Found"), 404);
      }

      if (!auth.organizations.includes(existing.organization)) {
        return c.json(failure("Access denied to organization"), 403);
      }

      const deleted = await service.delete(slug);
      if (!deleted) {
        return c.json(failure("Unable to delete project"), 500);
      }

      return c.json({ success: true, result: true });
    } catch (e: any) {
      return c.json(failure(e?.message ?? "Unable to delete project"), 500);
    }
  });

  // GET / (list)
  const listProjects = createRoute({
    method: "get",
    path: "/",
    request: {
      query: zOpenApi.object({
        page: zOpenApi.coerce.number().int().positive().optional(),
        per_page: zOpenApi.coerce.number().int().positive().max(100).optional(),
        search: zOpenApi.string().optional(),
        name: zOpenApi.string().optional(),
        slug: zOpenApi.string().optional(),
        organization: zOpenApi.string().optional(),
        orderBy: zOpenApi.enum(["name", "createdAt", "updatedAt"]).optional(),
      }),
    },
    responses: {
      200: {
        description: "Projects list",
        content: {
          "application/json": {
            schema: SuccessListEnvelopeOpenApi(zOpenApi.array(ProjectSchema)),
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
    tags: ["projects"],
    summary: "List Projects",
    description: "List projects with optional filters",
  } as const);

  app.openapi(listProjects, async (c) => {
    const q = c.req.valid("query");
    const service = new ProjectService(c.env.DB);
    const res = await service.list(q);

    return c.json({
      success: true,
      result: res.items,
      result_info: res.result_info,
    });
  });

  return app;
}
