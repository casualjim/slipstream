import { D1CreateEndpoint, D1DeleteEndpoint, D1ListEndpoint, D1ReadEndpoint, D1UpdateEndpoint } from "chanfana";
import { HTTPException } from "hono/http-exception";
import { ProjectService } from "../lib/services/projects";
import type { HandleArgs, Project } from "../types";
import { ProjectSchema } from "../types";

const projectMeta = {
  fields: ProjectSchema.pick({
    name: true,
    description: true,
    organization: true,
    slug: true,
  }),
  model: {
    tableName: "projects",
    primaryKeys: ["slug"],
    schema: ProjectSchema,
    serializer: (obj: Record<string, unknown>) => obj,
    serializerObject: ProjectSchema,
  },
  pathParameters: ["slug"],
};

export class CreateProject extends D1CreateEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = projectMeta;

  async before(data: Project): Promise<Project> {
    const [c] = this.args;
    const auth = c.get("auth");

    data.createdAt = new Date().toISOString();
    data.updatedAt = new Date().toISOString();

    // Validate org access
    if (!auth.organizations.includes(data.organization)) {
      throw new HTTPException(403, { message: "Access denied to organization" });
    }

    // Check slug uniqueness
    const service = new ProjectService(this.getDBBinding());
    if (await service.checkSlugExists(data.slug)) {
      throw new HTTPException(400, { message: "Slug already exists" });
    }

    return data;
  }
}

export class GetProject extends D1ReadEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = projectMeta;

  // Override to fix Chanfana bug with empty filters
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
    return obj.results[0];
  }
}

export class UpdateProject extends D1UpdateEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = {
    ...projectMeta,
    // Allow partial updates by making fields optional
    fields: ProjectSchema.pick({
      name: true,
      description: true,
      organization: true,
    }).partial(),
  };

  async before(oldObj: Record<string, any>, filters: any): Promise<any> {
    const [c] = this.args;
    const auth = c.get("auth");

    // Check access to existing project
    if (!auth.organizations.includes(oldObj.organization)) {
      throw new HTTPException(403, { message: "Access denied to organization" });
    }

    // Validate org access if organization is being updated
    if (filters.updatedData?.organization && !auth.organizations.includes(filters.updatedData.organization)) {
      throw new HTTPException(403, { message: "Access denied to organization" });
    }

    // Set updated metadata
    if (!filters.updatedData) {
      filters.updatedData = {};
    }
    filters.updatedData.updatedAt = new Date().toISOString();

    return filters;
  }

  // Override to fix Chanfana bug with empty filters
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
    return obj.results[0];
  }
}

export class DeleteProject extends D1DeleteEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = projectMeta;

  async before(data: any, filters: any): Promise<any> {
    const [c] = this.args;
    const auth = c.get("auth");

    // Get the existing project to check authorization
    const existing = await this.fetch(filters);
    if (!existing) {
      throw new HTTPException(404, { message: "Project not found" });
    }

    // Ensure user has access to the project's organization
    if (!auth.organizations.includes(existing.organization as string)) {
      throw new HTTPException(403, { message: "Access denied to organization" });
    }

    return filters;
  }

  // Override to fix Chanfana bug with empty filters
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
    return obj.results[0];
  }
}

export class ListProjects extends D1ListEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = {
    ...projectMeta,
    filterFields: ["name", "slug", "organization"],
  };
}
