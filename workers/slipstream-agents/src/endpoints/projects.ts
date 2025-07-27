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
    slug: true, // Add slug to fields for path parameter extraction
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

    // Generate slug from name
    const slug = data.name
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, "-")
      .replace(/^-+|-+$/g, "")
      .substring(0, 100);

    data.slug = slug;
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
  _meta = projectMeta;
}

export class DeleteProject extends D1DeleteEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = projectMeta;
}

export class ListProjects extends D1ListEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = {
    ...projectMeta,
    filterFields: ["name", "slug", "organization"],
  };
}
