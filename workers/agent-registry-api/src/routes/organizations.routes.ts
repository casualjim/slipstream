import { createRoute, OpenAPIHono, z as zOpenApi } from "@hono/zod-openapi";
import { z } from "zod";
import { generateSlug, isValidSlug } from "../lib/utils";
import { ErrorEnvelopeOpenApi, failure, SuccessEnvelopeOpenApi, SuccessListEnvelopeOpenApi } from "../schemas/envelope";
import {
  CreateOrganizationSchema,
  OrganizationSchema,
  OrganizationService,
  UpdateOrganizationSchema,
} from "../services/organization.service";
import type { AppHonoEnv } from "../types";

// Param schemas
const SlugParam = z.string().min(1);

// Factory
export function createOrganizationsRouter() {
  const app = new OpenAPIHono<AppHonoEnv>();

  // POST /
  const postOrg = createRoute({
    method: "post",
    path: "/",
    request: {
      body: {
        description: "Create a new Organization",
        content: {
          "application/json": {
            schema: zOpenApi.object({
              ...CreateOrganizationSchema.shape,
            }),
          },
        },
        required: true,
      },
    },
    responses: {
      201: {
        description: "Organization created",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(OrganizationSchema),
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
      409: {
        description: "Conflict - Duplicate slug",
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
    tags: ["organizations"],
    summary: "Create Organization",
    description: "Creates a new organization in the registry",
  } as const);

  app.openapi(postOrg, async (c) => {
    try {
      const body = await c.req.json();
      const input = await CreateOrganizationSchema.parseAsync(body);
      const db = c.env.DB;

      // Derive or normalize slug using shared utils
      const desiredSlug = input.slug ?? generateSlug(input.name);

      // Validate slug format using shared util
      if (!isValidSlug(desiredSlug)) {
        return c.json(failure("Invalid slug format"), 400);
      }

      // Proceed with creation using CRUD service (it will compute createdAt/updatedAt)
      const service = new OrganizationService(db);
      try {
        const created = await service.create({ ...input, slug: desiredSlug });

        const envelope: { success: true; result: z.infer<typeof OrganizationSchema> } = {
          success: true,
          result: created,
        };
        return c.json(envelope, 201);
      } catch (e: unknown) {
        const msg = String((e as any)?.message ?? "");
        // D1 unique violation format observed from test logs:
        // "D1_ERROR: UNIQUE constraint failed: organizations.slug: SQLITE_CONSTRAINT"
        if (msg.includes("UNIQUE constraint failed: organizations.slug")) {
          return c.json(failure("Slug already exists"), 409);
        }

        return c.json(failure("Unable to create organization"), 500);
      }
    } catch (err) {
      if (err && typeof err === "object" && "issues" in (err as any)) {
        const first = (err as any).issues?.[0];
        return c.json(failure(first?.message ?? "Invalid input", undefined, first?.path?.join?.(".")), 400);
      }
      return c.json(failure("Unable to create organization"), 500);
    }
  });

  // GET /{slug}
  const getOrg = createRoute({
    method: "get",
    path: "/{slug}",
    request: {
      params: zOpenApi.object({
        slug: SlugParam,
      }),
    },
    responses: {
      200: {
        description: "Organization found",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(OrganizationSchema),
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
        description: "Organization not found",
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
    tags: ["organizations"],
    summary: "Get Organization",
    description: "Retrieve an organization by slug",
  } as const);

  app.openapi(getOrg, async (c) => {
    try {
      const { slug } = c.req.valid("param");
      const service = new OrganizationService(c.env.DB);
      const found = await service.get(slug);
      if (!found) {
        return c.json(failure("Not Found"), 404);
      }
      return c.json(
        {
          success: true,
          result: found,
        },
        200,
      );
    } catch (e: any) {
      return c.json(failure(e?.message ?? "Unable to get organization"), 500);
    }
  });

  // PUT /{slug}
  const putOrg = createRoute({
    method: "put",
    path: "/{slug}",
    request: {
      params: zOpenApi.object({
        slug: SlugParam,
      }),
      body: {
        description: "Partial Organization update",
        content: {
          "application/json": {
            schema: zOpenApi.object({
              ...UpdateOrganizationSchema.shape,
            }),
          },
        },
        required: true,
      },
    },
    responses: {
      200: {
        description: "Organization updated",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(OrganizationSchema),
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
        description: "Organization not found",
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
    tags: ["organizations"],
    summary: "Update Organization",
    description: "Update an existing organization",
  } as const);

  app.openapi(putOrg, async (c) => {
    try {
      const { slug } = c.req.valid("param");
      const body = await c.req.json();
      const patch = await UpdateOrganizationSchema.parseAsync(body);
      const service = new OrganizationService(c.env.DB);
      const updated = await service.update(slug, patch);
      if (!updated) {
        return c.json(failure("Not Found"), 404);
      }
      return c.json(
        {
          success: true,
          result: updated,
        },
        200,
      );
    } catch (err) {
      if (err && typeof err === "object" && "issues" in (err as any)) {
        const first = (err as any).issues?.[0];
        return c.json(failure(first?.message ?? "Invalid input", undefined, first?.path?.join?.(".")), 400);
      }
      return c.json(failure("Unable to update organization"), 500);
    }
  });

  // DELETE /{slug}
  const deleteOrg = createRoute({
    method: "delete",
    path: "/{slug}",
    request: {
      params: zOpenApi.object({
        slug: SlugParam,
      }),
    },
    responses: {
      200: {
        description: "Organization deleted",
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
        description: "Organization not found",
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
    tags: ["organizations"],
    summary: "Delete Organization",
    description: "Delete a specific organization",
  } as const);

  app.openapi(deleteOrg, async (c) => {
    try {
      const { slug } = c.req.valid("param");
      const db = c.env.DB;

      const service = new OrganizationService(db);
      try {
        const deleted = await service.delete(slug);
        if (!deleted) {
          return c.json(failure("Not Found"), 404);
        }
        return c.json({ success: true, result: true });
      } catch (e: any) {
        const msg = String(e?.message ?? "");
        // Our trigger raises ABORT with this message; map to 400
        if (msg.includes("Cannot delete organization with existing projects")) {
          return c.json(failure("Cannot delete organization with existing projects"), 400);
        }
        return c.json(failure("Unable to delete organization"), 500);
      }
    } catch (e: any) {
      return c.json(failure(e?.message ?? "Unable to delete organization"), 500);
    }
  });

  // GET / (list)
  const listOrgs = createRoute({
    method: "get",
    path: "/",
    request: {
      query: zOpenApi.object({
        page: zOpenApi.coerce.number().int().positive().optional(),
        per_page: zOpenApi.coerce.number().int().positive().max(100).optional(),
        search: zOpenApi.string().optional(),
        name: zOpenApi.string().optional(),
        orderBy: zOpenApi.enum(["name", "createdAt", "updatedAt"]).optional(),
      }),
    },
    responses: {
      200: {
        description: "Organizations list",
        content: {
          "application/json": {
            schema: SuccessListEnvelopeOpenApi(zOpenApi.array(OrganizationSchema)),
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
    tags: ["organizations"],
    summary: "List Organizations",
    description: "List organizations with optional filters",
  } as const);

  app.openapi(listOrgs, async (c) => {
    const q = c.req.valid("query");
    const service = new OrganizationService(c.env.DB);
    const res = await service.list({
      page: q.page,
      per_page: q.per_page,
      search: q.search,
      name: q.name,
      orderBy: q.orderBy,
    });
    return c.json(
      {
        success: true,
        result: res.items,
        result_info: res.result_info,
      },
      200,
    );
  });

  return app;
}
