import { z } from "zod";

// Shared regex and messages mirrored from existing conventions
const SLUG_REGEX = /^[A-Za-z0-9-]{3,}$/;
const NAME_REGEX = /^[A-Za-z0-9]+[\w\s]{2,}.*$/;
const SEMVER_REGEX =
  /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

const NAME_ERROR = "Name must contain at least 3 alphanumeric characters";
const SLUG_ERROR = "Slug must be at least 3 alphanumeric characters";
const DATE_ERROR = "Must be a valid ISO 8601 date string";
const SEMVER_ERROR = "Must be a valid semantic version without 'v' prefix (e.g., 1.2.3, 2.0.0-alpha.1)";

// Util schemas
const semver = z.string().regex(SEMVER_REGEX, { message: SEMVER_ERROR });
const isoDate = z
  .string()
  .trim()
  .refine((val) => !isNaN(Date.parse(val)), { message: DATE_ERROR });

// Enums
export const ToolProviderEnum = z.enum(["Client", "Local", "MCP", "Restate"]);

// Provider-specific configs
const RestateTypeSchema = z.enum(["service", "object", "workflow"]).default("service");

const RestateConfigSchema = z.object({
  service_name: z.string().trim().min(1, { message: "restate.service_name must not be empty" }),
  service_type: RestateTypeSchema,
});

const McpConfigStdioSchema = z.object({
  type: z.literal("stdio"),
  command: z.string().trim().min(1),
  args: z.array(z.string()).optional(),
  env: z.record(z.string(), z.string()).default({}),
});
const McpConfigSseSchema = z.object({
  type: z.literal("sse"),
  url: z.string().trim().min(1),
  headers: z.record(z.string(), z.string()).optional(),
});
const McpConfigHttpSchema = z.object({
  type: z.literal("http"),
  url: z.string().trim().min(1),
  headers: z.record(z.string(), z.string()).optional(),
});
const McpConfigSchema = z.discriminatedUnion("type", [McpConfigStdioSchema, McpConfigSseSchema, McpConfigHttpSchema]);

// Base Tool schema (effect-free)
export const ToolBase = z.object({
  slug: z.string().trim().regex(SLUG_REGEX, SLUG_ERROR),
  version: semver.describe("Semantic version (e.g., 1.0.0)"),
  provider: ToolProviderEnum,
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR),
  description: z.string().nullish(),
  arguments: z.record(z.string(), z.unknown()).nullish(),
  restate: RestateConfigSchema.optional(),
  mcp: McpConfigSchema.optional(),
  createdAt: isoDate,
  updatedAt: isoDate,
});

// Full Tool schema with cross-field validation
export const ToolSchema = ToolBase.superRefine((val, ctx) => {
  if (val.provider === "Restate") {
    if (!val.restate) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["restate"],
        message: "restate configuration is required when provider is 'Restate'",
      });
    }
    if (val.mcp) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["mcp"],
        message: "mcp configuration must not be provided unless provider is 'MCP'",
      });
    }
  } else if (val.provider === "MCP") {
    if (!val.mcp) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["mcp"],
        message: "mcp configuration is required when provider is 'MCP'",
      });
    }
    if (val.restate) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["restate"],
        message: "restate configuration must not be provided unless provider is 'Restate'",
      });
    }
  } else {
    if (val.restate) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["restate"],
        message: "restate configuration must not be provided unless provider is 'Restate'",
      });
    }
    if (val.mcp) {
      ctx.addIssue({
        code: z.ZodIssueCode.custom,
        path: ["mcp"],
        message: "mcp configuration must not be provided unless provider is 'MCP'",
      });
    }
  }
});

// Create/Update payload schemas
export const CreateToolSchema = ToolBase.pick({
  slug: true,
  version: true,
  provider: true,
  name: true,
  description: true,
  arguments: true,
  restate: true,
  mcp: true,
}).extend({
  slug: ToolBase.shape.slug.optional(),
  createdAt: z.never().optional(),
  updatedAt: z.never().optional(),
});

export const UpdateToolSchema = z.object({
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR).optional(),
  description: z.string().optional(),
  arguments: z.record(z.string(), z.unknown()).optional(),
  restate: RestateConfigSchema.optional(),
  mcp: McpConfigSchema.optional(),
});

// Response envelope and error formats
export const ErrorItemSchema = z.object({
  message: z.string(),
  code: z.string().optional(),
  field: z.string().optional(),
});
export const ErrorEnvelopeSchema = z.object({
  success: z.literal(false),
  errors: z.array(ErrorItemSchema),
});
export const SuccessEnvelopeSchema = z.object({
  success: z.literal(true),
  // Result is intentionally unknown here; concrete routes will narrow it
  result: z.any(),
  // Optional pagination metadata used by list endpoints
  result_info: z
    .object({
      count: z.number().int().nonnegative(),
      page: z.number().int().nonnegative().optional(),
      per_page: z.number().int().nonnegative().optional(),
      total_count: z.number().int().nonnegative().optional(),
    })
    .optional(),
});

export type Tool = z.infer<typeof ToolSchema>;
export type CreateToolInput = z.infer<typeof CreateToolSchema>;
export type UpdateToolInput = z.infer<typeof UpdateToolSchema>;
