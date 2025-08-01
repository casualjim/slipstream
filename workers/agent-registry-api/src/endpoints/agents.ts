import { D1CreateEndpoint, D1DeleteEndpoint, D1ListEndpoint, D1ReadEndpoint, D1UpdateEndpoint } from "chanfana";
import { HTTPException } from "hono/http-exception";
import { z } from "zod";
import { AgentService, ModelService, ProjectService, ToolService } from "../lib/services";
import { generateSlug } from "../lib/utils";
import type { Agent, HandleArgs } from "../types";
import { AgentSchema, semverSchema } from "../types";

// Schema for API input, using array of strings for tools
const InputAgentSchema = z.object({
  name: AgentSchema.shape.name,
  version: AgentSchema.shape.version,
  description: AgentSchema.shape.description,
  model: AgentSchema.shape.model,
  instructions: AgentSchema.shape.instructions,
  availableTools: z
    .array(z.string())
    .nullish()
    .describe("A list of tools available to the agent, identified by their composite key."),
  organization: AgentSchema.shape.organization,
  project: AgentSchema.shape.project,
});

const agentMeta = {
  fields: InputAgentSchema,
  model: {
    schema: AgentSchema, // Uses existing schema with jsonArray for DB storage
    primaryKeys: ["slug", "version"],
    tableName: "agents",
    serializer: (obj: Record<string, unknown>) => {
      // Convert JSON strings back to arrays for the response
      if (obj.availableTools && typeof obj.availableTools === "string") {
        try {
          obj.availableTools = JSON.parse(obj.availableTools as string);
        } catch {
          console.error(`Failed to parse availableTools for agent ${obj.slug}:`, obj.availableTools);
          obj.availableTools = [];
        }
      }
      return obj;
    },
    serializerObject: AgentSchema,
  },
  pathParameters: ["slug", "version"],
};

/**
 * ## Create Agent
 *
 * Creates a new agent within a specified organization and project.
 * The agent's version must be a valid semantic version, and its slug is
 * automatically generated from the name. It validates that the associated
 * project, model, and tools exist and that the user has access to the
 * specified organization.
 */
export class CreateAgent extends D1CreateEndpoint<HandleArgs> {
  // @ts-expect-error - chanfana has poor type definitions
  _meta = {
    summary: "Create a new Agent",
    description: "Creates a new agent in the registry",
    ...agentMeta,
  };

  async before(data: any): Promise<any> {
    const [c] = this.args;
    const auth = c.get("auth");

    // Validate version format
    if (!semverSchema.safeParse(data.version).success) {
      throw new HTTPException(400, { message: "Invalid semantic version" });
    }

    // Generate slug from name
    const slug = generateSlug(data.name);

    // Set generated fields
    data.slug = slug;
    data.createdBy = auth.userId;
    data.updatedBy = auth.userId;
    data.createdAt = new Date().toISOString();
    data.updatedAt = new Date().toISOString();

    console.log("[CreateAgent.before] incoming:", {
      name: data.name,
      slug: data.slug,
      version: data.version,
      organization: data.organization,
      project: data.project,
      model: data.model,
    });

    // Validate access
    if (!auth.organizations.includes(data.organization)) {
      throw new HTTPException(403, { message: "Access denied to organization" });
    }

    // Validate project belongs to org
    const projectService = new ProjectService(this.getDBBinding());
    if (!(await projectService.belongsToOrganization(data.project, data.organization))) {
      throw new HTTPException(400, { message: "Project does not belong to organization" });
    }

    // Validate model exists
    const modelService = new ModelService(this.getDBBinding());
    if (!(await modelService.exists(data.model))) {
      throw new HTTPException(400, { message: "Invalid model ID" });
    }

    // Validate tools if provided
    if (data.availableTools?.length) {
      const toolService = new ToolService(this.getDBBinding());
      if (!(await toolService.validateIds(data.availableTools))) {
        throw new HTTPException(400, { message: "Invalid tool IDs" });
      }
    }

    // Convert availableTools array to JSON string for database storage
    if (data.availableTools !== undefined) {
      data.availableTools = data.availableTools?.length ? JSON.stringify(data.availableTools) : null;
    }

    return data;
  }

  // Override handle to catch constraint violations at a higher level
  async handle(c: any) {
    try {
      return await super.handle(c);
    } catch (error: any) {
      // Check if it's a unique constraint violation
      if (
        error?.message?.includes("UNIQUE constraint failed") ||
        error?.message?.includes("SQLITE_CONSTRAINT") ||
        error?.message?.includes("constraint")
      ) {
        throw new HTTPException(409, {
          message: "An agent with this name already exists in this organization and project",
        });
      }
      // Re-throw other errors
      throw error;
    }
  }
}

/**
 * ## Get Agent
 *
 * Retrieves a specific version of an agent by its slug and version.
 * It ensures that the user has access to the organization the agent belongs to.
 * This endpoint handles composite primary keys to fetch the correct agent.
 */
export class GetAgent extends D1ReadEndpoint<HandleArgs> {
  // @ts-expect-error - chanfana has poor type definitions
  _meta = {
    summary: "Get a specific Agent",
    description: "Retrieves a single agent by its slug and version from the registry",
    ...agentMeta,
  };

  // Override to fix Chanfana bug with empty filters and handle composite keys
  async fetch(filters: any) {
    if (!filters.filters || filters.filters.length === 0) {
      console.log("[GetAgent] No filters provided");
      return null;
    }

    const conditions = filters.filters.map((obj: any) => `${obj.field} = ?`);
    const values = filters.filters.map((obj: any) => obj.value);

    // Log the query and values
    console.log(
      "[GetAgent] Query:",
      `SELECT * FROM ${this.meta.model.tableName} WHERE ${conditions.join(" AND ")} LIMIT 1`,
    );
    console.log("[GetAgent] Values:", values);

    const obj = await this.getDBBinding()
      .prepare(`SELECT * FROM ${this.meta.model.tableName} WHERE ${conditions.join(" AND ")} LIMIT 1`)
      .bind(...values)
      .all<Agent>();

    // Log the DB results
    console.log("[GetAgent] DB Results:", obj.results);

    if (!obj.results || obj.results.length === 0) {
      console.log("[GetAgent] Agent not found");
      throw new HTTPException(404, { message: "Agent not found" });
    }

    const result = obj.results[0];

    // Log the result before auth check
    console.log("[GetAgent] Found agent:", result);

    // Check authorization
    const [c] = this.args;
    const auth = c.get("auth");
    if (!auth.organizations.includes(result.organization as string)) {
      console.log("[GetAgent] Access denied for org:", result.organization);
      throw new HTTPException(403, { message: "Access denied" });
    }

    return result;
  }
}

/**
 * ## Update Agent
 *
 * Updates an existing agent's properties.
 * This endpoint allows for partial updates of fields like name, description,
 * model, instructions, and available tools. It validates access to the agent
 * and ensures that any updated model or tool IDs are valid.
 */
export class UpdateAgent extends D1UpdateEndpoint<HandleArgs> {
  // @ts-expect-error - chanfana has poor type definitions
  _meta = {
    summary: "Update an existing Agent",
    description: "Updates an agent in the registry",
    ...agentMeta,
    fields: InputAgentSchema.pick({
      name: true,
      description: true,
      model: true,
      instructions: true,
      availableTools: true,
    }).partial(),
  };

  async before(oldObj: Record<string, any>, filters: any): Promise<any> {
    const [c] = this.args;
    const auth = c.get("auth");

    // Check access to existing agent
    if (!auth.organizations.includes(oldObj.organization)) {
      throw new HTTPException(403, { message: "Access denied" });
    }

    // Validate model if being updated
    if (filters.updatedData?.model) {
      const modelService = new ModelService(this.getDBBinding());
      if (!(await modelService.exists(filters.updatedData.model))) {
        throw new HTTPException(400, { message: "Invalid model ID" });
      }
    }

    // Validate tools if being updated
    if (filters.updatedData?.availableTools) {
      const toolService = new ToolService(this.getDBBinding());
      if (!(await toolService.validateIds(filters.updatedData.availableTools))) {
        throw new HTTPException(400, { message: "Invalid tool IDs" });
      }
    }

    // Convert availableTools array to JSON string for database storage
    if (filters.updatedData?.availableTools !== undefined) {
      filters.updatedData.availableTools = filters.updatedData.availableTools?.length
        ? JSON.stringify(filters.updatedData.availableTools)
        : null;
    }

    // Set updated metadata
    if (!filters.updatedData) {
      filters.updatedData = {};
    }
    filters.updatedData.updatedBy = auth.userId;
    filters.updatedData.updatedAt = new Date().toISOString();

    return filters;
  }
}

/**
 * ## Delete Agent
 *
 * Deletes a specific version of an agent from the registry.
 * It verifies that the user has the necessary permissions to delete the agent
 * by checking their access to the associated organization.
 */
export class DeleteAgent extends D1DeleteEndpoint<HandleArgs> {
  // @ts-expect-error - chanfana has poor type definitions
  _meta = {
    summary: "Delete an Agent",
    description: "Deletes an agent from the registry",
    ...agentMeta,
  };

  async before(oldObj: Record<string, any>, filters: any): Promise<any> {
    const [c] = this.args;
    const auth = c.get("auth");

    // Check access to existing agent
    if (!auth.organizations.includes(oldObj.organization)) {
      throw new HTTPException(403, { message: "Access denied" });
    }

    return filters;
  }
}

/**
 * ## List Agents
 *
 * Retrieves a paginated list of agents, filtered by the user's organization access.
 * It supports filtering by name, model, organization, and project, as well as
 * searching by name and description. The results are ordered and paginated.
 */
export class ListAgents extends D1ListEndpoint<HandleArgs> {
  // @ts-expect-error - chanfana has poor type definitions
  _meta = {
    summary: "List all Agents",
    description: "Retrieves a list of all agents in the registry",
    ...agentMeta,
  };

  filterFields = ["name", "model", "organization", "project"];
  searchFields = ["name", "description"];
  // @ts-ignore - chanfana has poor type definitions
  orderByFields = ["name", "createdAt", "updatedAt"];
  defaultOrderBy = "createdAt";

  async list() {
    const [c] = this.args;
    const auth = c.get("auth");
    const { organization, project, page = 1, per_page = 10 } = c.req.query();

    // Validate pagination parameters
    const pageNum = Math.max(1, parseInt(String(page), 10));
    const pageSize = Math.max(1, Math.min(100, parseInt(String(per_page), 10)));
    const offset = (pageNum - 1) * pageSize;

    // Build WHERE clause to filter by user's organizations
    let whereClause = `organization IN (${auth.organizations.map(() => "?").join(",")})`;
    const params = [...auth.organizations];

    if (organization && auth.organizations.includes(organization)) {
      whereClause += " AND organization = ?";
      params.push(organization);
    }

    if (project) {
      whereClause += " AND project = ?";
      params.push(project);
    }

    // Get total count for pagination metadata
    const countResult = await this.getDBBinding()
      .prepare(`SELECT COUNT(*) as total FROM agents WHERE ${whereClause}`)
      .bind(...params)
      .first();

    const totalCount = countResult?.total || 0;

    // Get paginated results
    const result = await this.getDBBinding()
      .prepare(`SELECT * FROM agents WHERE ${whereClause} ORDER BY createdAt DESC LIMIT ? OFFSET ?`)
      .bind(...params, pageSize, offset)
      .all();

    // Transform results to handle JSON fields
    const transformedResults = (result.results || []).map(agentMeta.model.serializer);

    return {
      result: transformedResults,
      success: true,
      result_info: {
        page: pageNum,
        per_page: pageSize,
        count: transformedResults.length,
        total_count: totalCount,
        total_pages: Math.ceil(Number(totalCount) / pageSize),
      },
    };
  }
}

/**
 * ## Get Latest Agent
 *
 * Retrieves the latest version of an agent by its slug.
 * It ensures that the user has access to the organization the agent belongs to.
 * This endpoint finds the most recent semantic version of the agent.
 */
export class GetLatestAgent extends D1ReadEndpoint<HandleArgs> {
  // @ts-expect-error - chanfana has poor type definitions
  _meta = {
    summary: "Get the latest version of an Agent",
    description: "Retrieves the latest version of an agent by its slug from the registry",
    ...agentMeta,
    // Explicitly expose slug to the read endpoint so Chanfana maps :slug into filters
    fields: AgentSchema.pick({ slug: true }),
    pathParameters: ["slug"], // Only slug, no version
  };

  // Override to handle fetching the latest version
  async fetch(filters: any) {
    if (!filters.filters || filters.filters.length === 0) {
      console.log("[GetLatestAgent] No filters provided");
      throw new HTTPException(404, { message: "Agent not found" });
    }

    console.log("[GetLatestAgent] Filters:", filters.filters);

    // Extract slug from filters
    const slugFilter = filters.filters.find((f: any) => f.field === "slug");
    if (!slugFilter) {
      console.log("[GetLatestAgent] No slug filter found");
      throw new HTTPException(404, { message: "Agent not found" });
    }

    const slug = slugFilter.value;
    console.log("[GetLatestAgent] Looking for latest version of slug:", slug);

    // Use AgentService to get the latest version
    const agentService = new AgentService(this.getDBBinding());
    const agent = await agentService.getLatestBySlug(slug);

    if (!agent) {
      console.log("[GetLatestAgent] Agent not found");
      throw new HTTPException(404, { message: "Agent not found" });
    }

    console.log("[GetLatestAgent] Found agent:", agent);

    // Check authorization
    const [c] = this.args;
    const auth = c.get("auth");
    if (!auth.organizations.includes(agent.organization as string)) {
      console.log("[GetLatestAgent] Access denied for org:", agent.organization);
      throw new HTTPException(403, { message: "Access denied" });
    }

    return agent;
  }
}
