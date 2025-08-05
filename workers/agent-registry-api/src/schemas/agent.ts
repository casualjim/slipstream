import { z } from "zod";
import { semverSchema } from "../types";

// Reuse shared regex and date/semver patterns consistent with tools
const SLUG_REGEX = /^[A-Za-z0-9-]{3,}$/;
const NAME_REGEX = /^[A-Za-z0-9]+[\w\s]{2,}.*$/;
// Tool ID format: provider/slug/version (e.g., "Local/my-tool/1.0.0")
const TOOL_ID_REGEX = /^(Client|Local|MCP|Restate)\/[A-Za-z0-9-]{3,}\/(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)\.(?:0|[1-9]\d*)(?:-[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?(?:\+[0-9A-Za-z-]+(?:\.[0-9A-Za-z-]+)*)?$/;

const NAME_ERROR = "Name must contain at least 3 alphanumeric characters";
const SLUG_ERROR = "Slug must be at least 3 alphanumeric characters";
const TOOL_ID_ERROR = "Tool ID must be in format 'provider/slug/version' (e.g., 'Local/my-tool/1.0.0')";
const DATE_ERROR = "Must be a valid ISO 8601 date string";

const isoDate = z
  .string()
  .trim()
  .refine((val) => !isNaN(Date.parse(val)), { message: DATE_ERROR });

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
  availableTools: z.array(z.string().trim().regex(TOOL_ID_REGEX, TOOL_ID_ERROR))
    .nullish()
    .describe("A list of tools available to the agent, identified by provider/slug/version format (e.g., 'Local/my-tool/1.0.0')."),
  organization: z.string().trim().min(1).describe("The slug of the organization this agent belongs to."),
  project: z.string().trim().min(1).describe("The slug of the project this agent belongs to."),
  createdBy: z.string().trim().min(1).describe("The ID of the user who created the agent."),
  updatedBy: z.string().trim().min(1).describe("The ID of the user who last updated the agent."),
  createdAt: isoDate.describe("The date and time when the agent was created, in ISO 8601 format."),
  updatedAt: isoDate.describe("The date and time when the agent was last updated, in ISO 8601 format."),
});

export const CreateAgentSchema = AgentSchema.pick({
  slug: true,
  version: true,
  name: true,
  description: true,
  model: true,
  instructions: true,
  availableTools: true,
  organization: true,
  project: true,
}).extend({
  slug: AgentSchema.shape.slug.optional(), // will be generated from name if omitted
  createdBy: AgentSchema.shape.createdBy.optional(), // will be generated from name if omitted
  updatedBy: AgentSchema.shape.updatedBy.optional(), // will be generated from name if omitted
});

export const UpdateAgentSchema = z.object({
  name: z.string().trim().regex(NAME_REGEX, NAME_ERROR).optional(),
  description: z.string().optional(),
  model: z.string().trim().min(1).optional(),
  instructions: z.string().trim().min(1).optional(),
  availableTools: z.array(z.string()).optional(),
  // organization and project are immutable for existing composite identity in this API
});

export type Agent = z.infer<typeof AgentSchema>;
export type CreateAgentInput = z.infer<typeof CreateAgentSchema>;
export type UpdateAgentInput = z.infer<typeof UpdateAgentSchema>;
