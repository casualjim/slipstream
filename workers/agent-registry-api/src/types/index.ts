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
  slug: z
    .string()
    .trim()
    .regex(SLUG_REGEX, SLUG_ERROR)
    .describe(
      "A unique identifier for the organization, used in URLs. It must be at least 3 characters long and can only contain alphanumeric characters and hyphens.",
    ),
  name: z
    .string()
    .trim()
    .regex(NAME_REGEX, NAME_ERROR)
    .describe(
      "The full name of the organization. It must be at least 3 characters long and can contain alphanumeric characters and spaces.",
    ),
  description: z.string().describe("A brief description of the organization.").optional(),
  createdAt: isoDate.describe("The date and time when the organization was created, in ISO 8601 format."),
  updatedAt: isoDate.describe("The date and time when the organization was last updated, in ISO 8601 format."),
});

/**
 * Project entity schema
 */
export const ProjectSchema = z.object({
  slug: z
    .string()
    .trim()
    .regex(SLUG_REGEX, SLUG_ERROR)
    .describe(
      "A unique identifier for the project, used in URLs. It must be at least 3 characters long and can only contain alphanumeric characters and hyphens.",
    ),
  name: z
    .string()
    .trim()
    .regex(NAME_REGEX, NAME_ERROR)
    .describe(
      "The full name of the project. It must be at least 3 characters long and can contain alphanumeric characters and spaces.",
    ),
  description: z.string().nullish().describe("A brief description of the project."),
  organization: z.string().trim().min(1).describe("The slug of the organization this project belongs to."),
  createdAt: isoDate.describe("The date and time when the project was created, in ISO 8601 format."),
  updatedAt: isoDate.describe("The date and time when the project was last updated, in ISO 8601 format."),
});

/**
 * User entity schema
 */
export const UserSchema = z.object({
  id: z.string().trim().min(1).describe("A unique identifier for the user."),
  name: z.string().trim().min(1).describe("The full name of the user."),
  email: z.string().trim().min(1).describe("The user's email address."),
  organizations: jsonArray.describe("A list of organization slugs that the user belongs to."),
  createdAt: isoDate.describe("The date and time when the user was created, in ISO 8601 format."),
  updatedAt: isoDate.describe("The date and time when the user was last updated, in ISO 8601 format."),
});

/**
 * Tool entity schema
 */
export const ToolSchema = z.object({
  slug: z
    .string()
    .trim()
    .regex(SLUG_REGEX, SLUG_ERROR)
    .describe(
      "A unique identifier for the tool, used in URLs. It must be at least 3 characters long and can only contain alphanumeric characters and hyphens.",
    ),
  version: semverSchema.describe("The semantic version of the tool (e.g., 1.0.0)."),
  provider: z.nativeEnum(ToolProvider).describe("The provider of the tool, indicating where it is executed."),
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR).describe("The display name of the tool."),
  description: z.string().nullish().describe("A brief description of what the tool does."),
  arguments: z.record(z.unknown()).nullish().describe("The JSON schema for the tool's arguments."), // JSON Schema object
  createdAt: isoDate.describe("The date and time when the tool was created, in ISO 8601 format."),
  updatedAt: isoDate.describe("The date and time when the tool was last updated, in ISO 8601 format."),
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
  id: z.string().trim().min(1).describe("A unique identifier for the model provider."),
  name: z.string().trim().min(1).describe("The display name of the model provider."),
  provider: z.nativeEnum(Provider).describe("The name of the provider (e.g., OpenAI, Anthropic)."),
  description: z.string().nullish().describe("A brief description of the model provider."),
  contextSize: z.number().describe("The maximum number of tokens that can be processed in a single request."),
  maxTokens: z.number().nullish().describe("The maximum number of tokens that can be generated in a single response."),
  temperature: z.number().nullish().describe("Controls randomness in the generation process. Higher values mean more randomness."),
  topP: z.number().nullish().describe("Controls diversity via nucleus sampling. A lower value means less diversity."),
  frequencyPenalty: z.number().nullish().describe("Penalizes new tokens based on their existing frequency in the text so far."),
  presencePenalty: z.number().nullish().describe("Penalizes new tokens based on whether they appear in the text so far."),
  capabilities: jsonArray.describe("A list of the model's capabilities, such as chat, completion, or function calling."), // Array of ModelCapabilities
  inputModalities: jsonArray.describe("A list of the modalities the model can accept as input (e.g., text, image, video)."), // Array of Modalities
  outputModalities: jsonArray.describe("A list of the modalities the model can generate as output (e.g., text, image, video)."), // Array of Modalities
  dialect: z.nativeEnum(APIDialect).nullish().describe("The API dialect used by the model provider (e.g., openai, anthropic)."),
});

/**
 * Agent entity schema
 */
export const AgentSchema = z.object({
  slug: z
    .string()
    .trim()
    .regex(SLUG_REGEX, SLUG_ERROR)
    .describe(
      "A unique identifier for the agent, used in URLs. It must be at least 3 characters long and can only contain alphanumeric characters and hyphens.",
    ),
  version: semverSchema.describe("The semantic version of the agent (e.g., 1.0.0)."),
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR).describe("The display name of the agent."),
  description: z.string().nullish().describe("A brief description of what the agent does."),
  model: z.string().trim().min(1).describe("The identifier of the model used by the agent."),
  instructions: z.string().trim().min(1).describe("The system instructions or prompt for the agent."),
  availableTools: jsonArray.nullish().describe("A list of tools available to the agent, identified by slug and version."),
  organization: z.string().trim().min(1).describe("The slug of the organization this agent belongs to."),
  project: z.string().trim().min(1).describe("The slug of the project this agent belongs to."),
  createdBy: z.string().trim().min(1).describe("The ID of the user who created the agent."),
  updatedBy: z.string().trim().min(1).describe("The ID of the user who last updated the agent."),
  createdAt: isoDate.describe("The date and time when the agent was created, in ISO 8601 format."),
  updatedAt: isoDate.describe("The date and time when the agent was last updated, in ISO 8601 format."),
});

// Re-export schemas for easier imports
export type Organization = z.infer<typeof OrganizationSchema>;
export type Project = z.infer<typeof ProjectSchema>;
export type User = z.infer<typeof UserSchema>;
export type Tool = z.infer<typeof ToolSchema>;
export type ModelProvider = z.infer<typeof ModelProviderSchema>;
export type Agent = z.infer<typeof AgentSchema>;

// Use the generated Env interface directly
// Extend Env to include our Durable Object namespace for EventHub
// Extend Env to include our Durable Object namespace for EventHub
export interface AppEnv extends Env {
  // Durable Object binding for event streaming
  EVENT_HUB: any;
}

// Define our auth context type
export type AuthContext = {
  userId: string;
  organizations: string[];
};

// Define our app context and handle args
export type AppHonoEnv = { Bindings?: AppEnv; Variables?: { auth: AuthContext } };
export type AppContext = Context<AppHonoEnv>;
export type HandleArgs = [AppContext];
