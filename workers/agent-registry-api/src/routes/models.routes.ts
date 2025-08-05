import { createRoute, OpenAPIHono, z as zOpenApi } from "@hono/zod-openapi";
import { z } from "zod";
import {
  ErrorEnvelopeOpenApi,
  failure,
  SuccessEnvelopeOpenApi,
  SuccessListEnvelopeOpenApi,
} from "../schemas/envelope";
import type { AppHonoEnv } from "../types";
import { Modalities, ModelCapabilities, ModelProviderSchema } from "../types";

// Param schema
const IdParam = ModelProviderSchema.shape.id;

// DB row and serializer
type ModelDbRow = {
  id: string;
  name: string;
  provider: string;
  description: string | null;
  contextSize: number | null;
  maxTokens: number | null;
  temperature: number | null;
  topP: number | null;
  frequencyPenalty: number | null;
  presencePenalty: number | null;
  capabilities: string | string[] | null;
  inputModalities: string | string[] | null;
  outputModalities: string | string[] | null;
  dialect: string | null;
};

function parseArrayField(value: string | string[] | null): string[] {
  if (Array.isArray(value)) return value;
  if (typeof value === "string" && value.length) {
    try {
      const parsed = JSON.parse(value);
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  }
  return [];
}

function serializeModelRow(row: ModelDbRow) {
  return {
    id: row.id,
    name: row.name,
    provider: row.provider,
    description: row.description ?? undefined,
    contextSize: row.contextSize ?? 0,
    maxTokens: row.maxTokens ?? undefined,
    temperature: row.temperature ?? undefined,
    topP: row.topP ?? undefined,
    frequencyPenalty: row.frequencyPenalty ?? undefined,
    presencePenalty: row.presencePenalty ?? undefined,
    capabilities: parseArrayField(row.capabilities),
    inputModalities: parseArrayField(row.inputModalities),
    outputModalities: parseArrayField(row.outputModalities),
    dialect: row.dialect ?? undefined,
  };
}

// Response schema for OpenAPI
const ModelResponseOpenApi = zOpenApi.object({
  id: zOpenApi.string().min(1),
  name: zOpenApi.string().min(1),
  provider: zOpenApi.string().min(1),
  description: zOpenApi.string().optional(),
  contextSize: zOpenApi.number().int(),
  maxTokens: zOpenApi.number().int().optional(),
  temperature: zOpenApi.number().optional(),
  topP: zOpenApi.number().optional(),
  frequencyPenalty: zOpenApi.number().optional(),
  presencePenalty: zOpenApi.number().optional(),
  capabilities: zOpenApi.array(zOpenApi.nativeEnum(ModelCapabilities)),
  inputModalities: zOpenApi.array(zOpenApi.nativeEnum(Modalities)),
  outputModalities: zOpenApi.array(zOpenApi.nativeEnum(Modalities)),
  dialect: zOpenApi.string().optional(),
});

// Factory
export function createModelsRouter() {
  const app = new OpenAPIHono<AppHonoEnv>();

  // GET /:id - read single (register BEFORE list so it isn't shadowed)
  const getModel = createRoute({
    method: "get",
    // Use a greedy pattern so IDs containing '/' are captured intact.
    // Hono path param regex uses :name{regex}
    path: "/:id{.+}",
    request: {
      params: zOpenApi.object({
        id: zOpenApi.string().min(1).openapi({ description: "Model identifier, may contain '/'" }),
      }),
    },
    responses: {
      200: {
        description: "Model found",
        content: {
          "application/json": {
            schema: SuccessEnvelopeOpenApi(ModelResponseOpenApi),
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
        description: "Model not found",
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
    tags: ["models"],
    summary: "Get Model",
    description: "Retrieve a specific model by its id",
  } as const);

  app.openapi(getModel, async (c) => {
    const auth = c.req.header("Authorization");
    if (!auth) return c.json(failure("Unauthorized"), 401);

    try {
      // Preserve raw param including slashes; do not decode to avoid double-processing
      const rawId = c.req.param("id");
      console.log("[models.get] raw param id =", rawId);

      // Validate against ModelProviderSchema.id
      const parsed = z.object({ id: IdParam }).safeParse({ id: rawId });
      if (!parsed.success) {
        const first = parsed.error.issues?.[0];
        console.debug("[models.get] param validation failed:", first);
        return c.json(failure(first?.message ?? "Invalid params", undefined, first?.path?.join?.(".")), 400);
      }

      const id = parsed.data.id;
      console.debug("[models.get] validated id =", id);

      const db = c.env.DB;

      // Exact match, case-insensitive to be robust against case mismatches
      const row = await db
        .prepare("SELECT * FROM model_providers WHERE id = ? COLLATE NOCASE")
        .bind(id)
        .first<ModelDbRow>();

      console.debug("[models.get] db match found =", !!row);

      if (!row) {
        return c.json(failure("Not Found"), 404);
      }

      const item = serializeModelRow(row);
      const verify = ModelResponseOpenApi.safeParse(item);
      if (!verify.success) {
        const first = verify.error.issues?.[0];
        console.debug("[models.get] schema validation failed:", first);
        return c.json(failure(first?.message ?? "Invalid model data"), 500);
      }
      return c.json({ success: true, result: item });
    } catch (e: any) {
      console.debug("[models.get] unhandled error:", e?.message);
      return c.json(failure(e?.message ?? "Unable to get model"), 500);
    }
  });

  // GET / - list
  const listModels = createRoute({
    method: "get",
    path: "/",
    request: {
      query: zOpenApi.object({
        provider: zOpenApi.string().optional(),
        name: zOpenApi.string().optional(),
        search: zOpenApi.string().optional(),
        orderBy: zOpenApi.enum(["name", "provider", "createdAt"]).optional(),
        order: zOpenApi.enum(["asc", "desc"]).optional(),
        page: zOpenApi.coerce.number().int().positive().optional(),
        per_page: zOpenApi.coerce.number().int().positive().max(1000).optional(),
      }),
    },
    responses: {
      200: {
        description: "Models list",
        content: {
          "application/json": {
            schema: SuccessListEnvelopeOpenApi(zOpenApi.array(ModelResponseOpenApi)),
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
      400: {
        description: "Bad Request",
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
    tags: ["models"],
    summary: "List Models",
    description: "List models with optional filters, search and pagination",
  } as const);

  app.openapi(listModels, async (c) => {
    const auth = c.req.header("Authorization");
    if (!auth) {
      const envelope = failure("Unauthorized");
      return c.json(envelope, 401);
    }

    const q = c.req.valid("query");
    const db = c.env.DB;

    // Translate page/per_page to limit/offset
    const per = q.per_page ? Math.max(1, q.per_page) : undefined;
    const page = Math.max(1, q.page ?? 1);
    const offset = per ? (page - 1) * per : 0;

    const where: string[] = [];
    const bind: unknown[] = [];

    if (q.provider) {
      where.push("provider = ?");
      bind.push(q.provider);
    }
    if (q.name) {
      where.push(`LOWER(name) LIKE LOWER(?)`);
      bind.push(`%${q.name}%`);
    }
    if (q.search) {
      where.push(`(name LIKE ? OR description LIKE ?)`);
      bind.push(`%${q.search}%`, `%${q.search}%`);
    }

    const clauses = where.length ? `WHERE ${where.join(" AND ")}` : "";
    const orderBy = q.orderBy ?? "name";
    const order = (q.order ?? "asc").toUpperCase();
    const limitClause = per ? ` LIMIT ${per} OFFSET ${offset}` : "";

    try {
      const countRow = await db
        .prepare(`SELECT COUNT(*) as total FROM model_providers ${clauses}`)
        .bind(...bind)
        .first<{ total: number }>();
      const total_count = (countRow?.total ?? 0) as number;

      const sql = `
        SELECT * FROM model_providers
        ${clauses}
        ORDER BY ${orderBy} ${order}
        ${limitClause}
      `;
      const res = await db.prepare(sql).bind(...bind).all<ModelDbRow>();
      const rows = res.results ?? [];
      const items = rows.map((r) => serializeModelRow(r));

      const envelope: {
        success: true;
        result: ReturnType<typeof serializeModelRow>[];
        result_info: { count: number; page?: number; per_page?: number; total_count: number };
      } = {
        success: true,
        result: items,
        result_info: {
          count: items.length,
          page: per ? page : undefined,
          per_page: per ?? undefined,
          total_count,
        },
      };
      return c.json(envelope);
    } catch (e: any) {
      const envelope = failure(e?.message ?? "Unable to list models");
      return c.json(envelope, 500);
    }
  });


  // Important: do NOT expose create/update/delete for models per tests (read-only)
  return app;
}
