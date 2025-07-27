import { D1CreateEndpoint, D1DeleteEndpoint, D1ListEndpoint, D1ReadEndpoint, D1UpdateEndpoint } from "chanfana";
import { HTTPException } from "hono/http-exception";
import { z } from "zod";
import { ModelService, ProjectService, ToolService } from "../lib/services";
import type { HandleArgs } from "../types";
import { AgentSchema } from "../types";

const agentMeta = {
  fields: z.object({
    name: z.string(),
    version: z.string(),
    description: z.string().nullish(),
    model: z.string(),
    instructions: z.string(),
    availableTools: z.array(z.string()).nullish(),
    organization: z.string(),
    project: z.string(),
  }),
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
          obj.availableTools = [];
        }
      }
      return obj;
    },
    serializerObject: AgentSchema,
  },
  pathParameters: ["slug", "version"],
};

export class CreateAgent extends D1CreateEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = agentMeta;

  async before(data: any): Promise<any> {
    const [c] = this.args;
    const auth = c.get("auth");

    // Generate slug from name
    const slug = data.name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .substring(0, 100);

    // Set generated fields
    data.slug = slug;
    data.createdBy = auth.userId;
    data.updatedBy = auth.userId;
    data.createdAt = new Date().toISOString();
    data.updatedAt = new Date().toISOString();

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
}

export class GetAgent extends D1ReadEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = agentMeta;

  // Override to fix Chanfana bug with empty filters and handle composite keys
  async fetch(filters: any) {
    if (!filters.filters || filters.filters.length === 0) {
      return null;
    }

    const conditions = filters.filters.map((obj: any) => `${obj.field} = ?`);
    const values = filters.filters.map((obj: any) => obj.value);

    const obj = await this.getDBBinding()
      .prepare(`SELECT * FROM ${this.meta.model.tableName} WHERE ${conditions.join(" AND ")} LIMIT 1`)
      .bind(...values)
      .all();

    if (!obj.results || obj.results.length === 0) {
      return null;
    }

    const result = obj.results[0] as any;

    // Check authorization
    const [c] = this.args;
    const auth = c.get("auth");
    if (!auth.organizations.includes(result.organization as string)) {
      throw new HTTPException(403, { message: "Access denied" });
    }

    return result;
  }
}

export class UpdateAgent extends D1UpdateEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = {
    ...agentMeta,
    fields: z.object({
      name: z.string(),
      description: z.string().nullish(),
      model: z.string(),
      instructions: z.string(),
      availableTools: z.array(z.string()).nullish(),
    }).partial(), // Allow partial updates but exclude primary key fields
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

export class DeleteAgent extends D1DeleteEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = agentMeta;

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

export class ListAgents extends D1ListEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = {
    ...agentMeta,
    filterFields: ["name", "model", "organization", "project"],
    searchFields: ["name", "description"],
  };

  async list() {
    const [c] = this.args;
    const auth = c.get("auth");
    const { organization, project, page = 1, per_page = 10 } = c.req.query();

    // Build WHERE clause to filter by user's organizations
    let whereClause = `organization IN (${auth.organizations.map(() => '?').join(',')})`;
    const params = [...auth.organizations];

    if (organization && auth.organizations.includes(organization)) {
      whereClause += " AND organization = ?";
      params.push(organization);
    }

    if (project) {
      whereClause += " AND project = ?";
      params.push(project);
    }

    const result = await this.getDBBinding()
      .prepare(`SELECT * FROM agents WHERE ${whereClause}`)
      .bind(...params)
      .all();

    // Transform results to handle JSON fields
    const transformedResults = (result.results || []).map(agentMeta.model.serializer);

    return {
      result: transformedResults,
      success: true,
      result_info: {
        page: Number(page),
        per_page: Number(per_page),
        count: transformedResults.length,
        total_count: transformedResults.length,
        total_pages: 1,
      },
    };
  }
}
