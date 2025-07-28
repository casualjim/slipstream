import type { Context } from "hono";
import { z } from "zod";

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

// Simple JSON array transformer for D1
const jsonArray = z.string().transform((val) => {
  try {
    return JSON.parse(val);
  } catch {
    return [];
  }
});

// Keep schemas minimal - let D1AutoEndpoint handle most validation
export const OrganizationSchema = z.object({
  slug: z.string().regex(/^[A-Za-z0-9-]{3,}$/),
  name: z.string().regex(/^[A-Za-z0-9]+[\w\s]{2,}.*$/, "Name must contain at least 3 alphanumeric characters"),
  description: z.string().optional(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const ProjectSchema = z.object({
  slug: z.string().regex(/^[A-Za-z0-9-]{3,}$/),
  name: z.string().regex(/^[A-Za-z0-9]+[\w\s]{2,}.*$/, "Name must contain at least 3 alphanumeric characters"),
  description: z.string().nullish(),
  organization: z.string(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const UserSchema = z.object({
  id: z.string(),
  name: z.string(),
  email: z.string(),
  organizations: jsonArray,
  createdAt: z.string(),
  updatedAt: z.string(),
});

export const ToolSchema = z.object({
  slug: z.string().regex(/^[A-Za-z0-9-_]{3,}$/),
  version: z.string(),
  provider: z.nativeEnum(ToolProvider),
  name: z.string().regex(/^[A-Za-z0-9]+[\w\s]{2,}.*$/, "Name must contain at least 3 alphanumeric characters"),
  description: z.string().nullish(),
  arguments: z.record(z.any()).nullish(), // JSON Schema object
  createdAt: z.string(),
  updatedAt: z.string(),
});

// Schema for creating tools - slug is optional and will be auto-generated
export const CreateToolSchema = z.object({
  slug: z.string().regex(/^[A-Za-z0-9-_]{3,}$/).optional(),
  version: z.string(),
  provider: z.nativeEnum(ToolProvider),
  name: z.string().regex(/^[A-Za-z0-9]+[\w\s]{2,}.*$/, "Name must contain at least 3 alphanumeric characters"),
  description: z.string().optional(),
  arguments: z.record(z.any()).optional(), // JSON Schema object

});

// Schema for updating tools - excludes primary keys
export const UpdateToolSchema = z.object({
  name: z.string().regex(/^[A-Za-z0-9]+[\w\s]{2,}.*$/, "Name must contain at least 3 alphanumeric characters").optional(),
  description: z.string().optional(),
  arguments: z.record(z.any()).optional(), // JSON Schema object
});

export const ModelProviderSchema = z.object({
  id: z.string(),
  name: z.string(),
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

export const AgentSchema = z.object({
  slug: z.string().regex(/^[A-Za-z0-9-_]{3,}$/),
  version: z.string(),
  name: z.string().regex(/^[A-Za-z0-9]{3,}/, "Name must contain at least 3 alphanumeric characters"),
  description: z.string().nullish(),
  model: z.string(),
  instructions: z.string(),
  availableTools: jsonArray.nullish(),
  organization: z.string(),
  project: z.string(),
  createdBy: z.string(),
  updatedBy: z.string(),
  createdAt: z.string(),
  updatedAt: z.string(),
});

// Re-export schemas for easier imports
export type Organization = z.infer<typeof OrganizationSchema>;
export type Project = z.infer<typeof ProjectSchema>;
export type User = z.infer<typeof UserSchema>;
export type Tool = z.infer<typeof ToolSchema>;
export type ModelProvider = z.infer<typeof ModelProviderSchema>;
export type Agent = z.infer<typeof AgentSchema>;

// Use the generated Env interface directly
export type AppEnv = Env;

// Define our auth context type
export type AuthContext = {
  userId: string;
  organizations: string[];
};

// Define our app context and handle args
export type AppHonoEnv = { Bindings?: AppEnv; Variables?: { auth: AuthContext } };
export type AppContext = Context<AppHonoEnv>;
export type HandleArgs = [AppContext];
