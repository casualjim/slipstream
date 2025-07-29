import type { Context } from "hono";
import { z } from "zod";

// Regex constants
const SLUG_REGEX = /^[A-Za-z0-9-]{3,}$/;
const NAME_REGEX = /^[A-Za-z0-9]+[\w\s]{2,}.*$/;
const SEMVER_REGEX = /^(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

// Error message constants
const NAME_ERROR = "Name must contain at least 3 alphanumeric characters";
const SLUG_ERROR = "Slug must be at least 3 alphanumeric characters";
const DATE_ERROR = "Must be a valid ISO 8601 date string";
const SEMVER_ERROR = "Must be a valid semantic version without 'v' prefix (e.g., 1.2.3, 2.0.0-alpha.1)";

// Constants for seed data
export const WAGYU_ORGANIZATION_SLUG = "wagyu";
export const WAGYU_PROJECT_SLUG = "wagyu-project";

// Enums
export enum ToolProvider {
  Client = "Client",
  Local = "Local",
  MCP = "MCP",
  Restate = "Restate",
}

export enum ModelCapabilities {
  CHAT = "chat",
  COMPLETION = "completion",
  EMBEDDINGS = "embeddings",
  RERANKING = "reranking",
  FUNCTION_CALLING = "function_calling",
  STRUCTURED_OUTPUT = "structured_output",
  CODE_EXECUTION = "code_execution",
  SEARCH = "search",
  THINKING = "thinking",
  IMAGE_GENERATION = "image_generation",
  VIDEO_GENERATION = "video_generation",
  CACHING = "caching",
  TUNING = "tuning",
  BATCH = "batch",
}

export enum Modalities {
  TEXT = "text",
  IMAGE = "image",
  VIDEO = "video",
  AUDIO = "audio",
  PDF = "pdf",
}

export enum APIDialect {
  OPENAI = "openai",
  ANTHROPIC = "anthropic",
  DEEPSEEK = "deepseek",
}

export enum Provider {
  OPENAI = "OpenAI",
  ANTHROPIC = "Anthropic",
  DEEPINFRA = "DeepInfra",
  GOOGLE = "Google",
  OPENROUTER = "OpenRouter",
}

// Strict array validation: only accept arrays, never stringified JSON
const jsonArray = z.array(z.unknown());

// Semver validation using regex (no 'v' prefix allowed)
export const semverSchema = z.string().regex(SEMVER_REGEX, { message: SEMVER_ERROR });

// ISO date validation
const isoDate = z.string().trim().refine((val) => !isNaN(Date.parse(val)), { message: DATE_ERROR });

/**
 * Organization entity schema
 */
export const OrganizationSchema = z.object({
  slug: z.string().trim().regex(SLUG_REGEX, SLUG_ERROR),
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR),
  description: z.string().optional(),
  createdAt: isoDate,
  updatedAt: isoDate,
});

/**
 * Project entity schema
 */
export const ProjectSchema = z.object({
  slug: z.string().trim().regex(SLUG_REGEX, SLUG_ERROR),
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR),
  description: z.string().nullish(),
  organization: z.string().trim().min(1),
  createdAt: isoDate,
  updatedAt: isoDate,
});

/**
 * User entity schema
 */
export const UserSchema = z.object({
  id: z.string().trim().min(1),
  name: z.string().trim().min(1),
  email: z.string().trim().min(1),
  organizations: jsonArray,
  createdAt: isoDate,
  updatedAt: isoDate,
});

/**
 * Tool entity schema
 */
export const ToolSchema = z.object({
  slug: z.string().trim().regex(SLUG_REGEX, SLUG_ERROR),
  version: semverSchema,
  provider: z.nativeEnum(ToolProvider),
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR),
  description: z.string().nullish(),
  arguments: z.record(z.unknown()).nullish(), // JSON Schema object
  createdAt: isoDate,
  updatedAt: isoDate,
});

/**
 * Schema for creating tools - slug is optional and will be auto-generated
 */
export const CreateToolSchema = z.object({
  slug: z.string().trim().regex(SLUG_REGEX, SLUG_ERROR).optional(),
  version: semverSchema,
  provider: z.nativeEnum(ToolProvider),
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR),
  description: z.string().optional(),
  arguments: z.record(z.unknown()).optional(), // JSON Schema object
});

/**
 * Schema for updating tools - excludes primary keys
 */
export const UpdateToolSchema = z.object({
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR).optional(),
  description: z.string().optional(),
  arguments: z.record(z.unknown()).optional(), // JSON Schema object
});

/**
 * Model provider entity schema
 */
export const ModelProviderSchema = z.object({
  id: z.string().trim().min(1),
  name: z.string().trim().min(1),
  provider: z.nativeEnum(Provider),
  description: z.string().nullish(),
  contextSize: z.number(),
  maxTokens: z.number().nullish(),
  temperature: z.number().nullish(),
  topP: z.number().nullish(),
  frequencyPenalty: z.number().nullish(),
  presencePenalty: z.number().nullish(),
  capabilities: jsonArray, // Array of ModelCapabilities
  inputModalities: jsonArray, // Array of Modalities
  outputModalities: jsonArray, // Array of Modalities
  dialect: z.nativeEnum(APIDialect).nullish(),
});

/**
 * Agent entity schema
 */
export const AgentSchema = z.object({
  slug: z.string().trim().regex(SLUG_REGEX, SLUG_ERROR),
  version: semverSchema,
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR),
  description: z.string().nullish(),
  model: z.string().trim().min(1),
  instructions: z.string().trim().min(1),
  availableTools: jsonArray.nullish(),
  organization: z.string().trim().min(1),
  project: z.string().trim().min(1),
  createdBy: z.string().trim().min(1),
  updatedBy: z.string().trim().min(1),
  createdAt: isoDate,
  updatedAt: isoDate,
});

// Re-export schemas for easier imports
export type Organization = z.infer<typeof OrganizationSchema>;
export type Project = z.infer<typeof ProjectSchema>;
export type User = z.infer<typeof UserSchema>;
export type Tool = z.infer<typeof ToolSchema>;
export type ModelProvider = z.infer<typeof ModelProviderSchema>;
export type Agent = z.infer<typeof AgentSchema>;

// Use the generated Env interface directly
export interface AppEnv extends Env {}

// Define our auth context type
export type AuthContext = {
  userId: string;
  organizations: string[];
};

// Define our app context and handle args
export type AppHonoEnv = { Bindings?: AppEnv; Variables?: { auth: AuthContext } };
export type AppContext = Context<AppHonoEnv>;
export type HandleArgs = [AppContext];
