import {
  D1CreateEndpoint,
  D1DeleteEndpoint,
  D1ListEndpoint,
  D1ReadEndpoint,
  D1UpdateEndpoint,
  type Filters,
} from "chanfana";
import { HTTPException } from "hono/http-exception";
import { OrganizationService } from "../lib/services/organizations";
import type { HandleArgs, Organization } from "../types";
import { OrganizationSchema } from "../types";

const orgMeta = {
  fields: OrganizationSchema.pick({
    // remove the createdAt, and updatedAt fields from the input, they are read-only
    // and set by the system
    // This allows D1AutoEndpoint to handle the rest of the validation
    // and serialization
    name: true,
    slug: true, // slug is already included, which is correct
    description: true,
  }),
  model: {
    tableName: "organizations",
    primaryKeys: ["slug"],
    schema: OrganizationSchema,
    serializer: (obj: Record<string, unknown>) => obj,
    serializerObject: OrganizationSchema,
  },
  pathParameters: ["slug"],
};

const readOnlyMeta = {
  fields: orgMeta.fields,  // Include fields for proper path parameter extraction
  model: orgMeta.model,
  pathParameters: orgMeta.pathParameters,
};

export class CreateOrganization extends D1CreateEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = orgMeta;

  async before(data: Organization): Promise<Organization> {
    // ALL database queries through service
    const service = new OrganizationService(this.getDBBinding());
    if (await service.checkSlugExists(data.slug)) {
      throw new HTTPException(400, { message: "Slug already exists" });
    }

    data.createdAt = new Date().toISOString();
    data.updatedAt = new Date().toISOString();

    return data;
  }
}

export class GetOrganization extends D1ReadEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = readOnlyMeta;

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

export class UpdateOrganization extends D1UpdateEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = orgMeta;
  // Simple CRUD - auth handled by middleware
}

export class DeleteOrganization extends D1DeleteEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = orgMeta;

  async before(_oldObj: Organization, filters: Filters): Promise<Filters> {
    const ff = filters.filters.find((f) => f.field === "slug");
    if (!ff || !ff.value) {
      throw new HTTPException(400, { message: "Missing organization slug" });
    }
    const slug = ff.value as string;

    // ALL database queries through service
    const service = new OrganizationService(this.getDBBinding());
    if (await service.hasProjects(slug)) {
      throw new HTTPException(400, {
        message: "Cannot delete organization with existing projects",
      });
    }

    return filters;
  }
}

export class ListOrganizations extends D1ListEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = {
    ...readOnlyMeta,
  };
  filterFields = ["name", "slug"];
  searchFields = ["name", "description"];
  // @ts-ignore - chanfana has poor type definitions
  orderByFields = ["name", "createdAt", "updatedAt"];
  defaultOrderBy = "name";
}
