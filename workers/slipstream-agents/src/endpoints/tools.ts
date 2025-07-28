import { D1CreateEndpoint, D1DeleteEndpoint, D1ListEndpoint, D1ReadEndpoint, D1UpdateEndpoint } from "chanfana";
import { generateSlug } from "../lib/utils";
import type { HandleArgs, Tool } from "../types";
import { ToolSchema } from "../types";

// Schema for creating tools - slug is optional since it can be auto-generated
const CreateToolSchema = ToolSchema.omit({ createdAt: true, updatedAt: true }).extend({
  slug: ToolSchema.shape.slug.optional(),
});

// Schema for updating tools - excludes primary keys and makes other fields optional
const UpdateToolSchema = ToolSchema.omit({
  slug: true,
  version: true,
  provider: true,
  createdAt: true,
  updatedAt: true,
}).partial();

const toolMeta = {
  fields: ToolSchema.pick({
    slug: true,
    version: true,
    provider: true,
    name: true,
    description: true,
    arguments: true,
  }),
  model: {
    schema: ToolSchema,
    primaryKeys: ["provider", "slug", "version"],
    tableName: "tools",
    serializer: (obj: Record<string, unknown>) => {
      // Convert JSON string back to object for the response
      if (obj.arguments && typeof obj.arguments === "string") {
        try {
          obj.arguments = JSON.parse(obj.arguments as string);
        } catch {
          // If parsing fails, keep as string
        }
      }

      // Remove null/undefined optional fields from the response
      if (obj.arguments === null || obj.arguments === undefined) {
        delete obj.arguments;
      }
      if (obj.description === null || obj.description === undefined) {
        delete obj.description;
      }

      return obj;
    },
    serializerObject: ToolSchema,
  },
  pathParameters: ["provider", "slug", "version"],
};
export class CreateTool extends D1CreateEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = {
    ...toolMeta,
    fields: CreateToolSchema,
  };

  async before(data: Tool): Promise<Tool> {
    data.createdAt = new Date().toISOString();
    data.updatedAt = new Date().toISOString();

    // Auto-generate slug from name if not provided
    if (!data.slug || data.slug.trim().length === 0) {
      data.slug = generateSlug(data.name);
    }

    // Convert arguments object to JSON string for storage
    if (data.arguments && typeof data.arguments === "object") {
      data.arguments = JSON.stringify(data.arguments) as any;
    } else if (data.arguments === undefined) {
      // D1 doesn't support undefined values, convert to null
      data.arguments = null as any;
    }

    // D1 doesn't support undefined values for description either
    if (data.description === undefined) {
      data.description = null as any;
    }

    return data;
  }
}

export class GetTool extends D1ReadEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = toolMeta;

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

export class UpdateTool extends D1UpdateEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = {
    ...toolMeta,
    fields: UpdateToolSchema,
  };

  async before(oldObj: Record<string, any>, filters: any): Promise<any> {
    // Convert arguments object to JSON string for storage in the updated data
    if (filters.updatedData?.arguments && typeof filters.updatedData.arguments === "object") {
      filters.updatedData.arguments = JSON.stringify(filters.updatedData.arguments);
    } else if (filters.updatedData?.arguments === undefined) {
      // D1 doesn't support undefined values, convert to null
      filters.updatedData.arguments = null;
    }

    // D1 doesn't support undefined values for description either
    if (filters.updatedData?.description === undefined) {
      filters.updatedData.description = null;
    }

    // Set updated timestamp
    if (!filters.updatedData) {
      filters.updatedData = {};
    }
    filters.updatedData.updatedAt = new Date().toISOString();

    return filters;
  }
}

export class DeleteTool extends D1DeleteEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = toolMeta;
}

export class ListTools extends D1ListEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = {
    model: toolMeta.model,
  };

  filterFields = ["name", "version", "provider", "slug"];
}
