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
    name: true,
    slug: true,
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

/**
 * ## Create Organization
 *
 * Creates a new organization.
 * It checks for slug uniqueness to ensure that each organization has a
 * distinct identifier.
 */
export class CreateOrganization extends D1CreateEndpoint<HandleArgs> {
  public static _meta = {
    summary: "Create a new Organization",
    description: "Creates a new organization in the registry",
    ...orgMeta,
  };

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

/**
 * ## Get Organization
 *
 * Retrieves a specific organization by its slug.
 */
export class GetOrganization extends D1ReadEndpoint<HandleArgs> {
  public static _meta = {
    summary: "Get a specific Organization",
    description: "Retrieves a single organization by its slug from the registry",
    ...readOnlyMeta,
  };

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

/**
 * ## Update Organization
 *
 * Updates an existing organization's properties.
 * Authentication is handled by middleware.
 */
export class UpdateOrganization extends D1UpdateEndpoint<HandleArgs> {
  public static _meta = {
    summary: "Update an existing Organization",
    description: "Updates an organization in the registry",
    ...orgMeta,
  };
  // Simple CRUD - auth handled by middleware
}

/**
 * ## Delete Organization
 *
 * Deletes an organization from the registry.
 * It prevents deletion if the organization still has associated projects.
 */
export class DeleteOrganization extends D1DeleteEndpoint<HandleArgs> {
  public static _meta = {
    summary: "Delete an Organization",
    description: "Deletes an organization from the registry",
    ...orgMeta,
  };

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

/**
 * ## List Organizations
 *
 * Retrieves a list of all organizations.
 * Supports filtering, searching, and ordering.
 */
export class ListOrganizations extends D1ListEndpoint<HandleArgs> {
  public static _meta = {
    summary: "List all Organizations",
    description: "Retrieves a list of all organizations in the registry",
    ...readOnlyMeta,
  };
  filterFields = ["name", "slug"];
  searchFields = ["name", "description"];
  // @ts-ignore - chanfana has poor type definitions
  orderByFields = ["name", "createdAt", "updatedAt"];
  defaultOrderBy = "name";
}
