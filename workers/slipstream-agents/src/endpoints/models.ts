import { D1ListEndpoint, D1ReadEndpoint } from "chanfana";
import type { HandleArgs } from "../types";
import { ModelProviderSchema } from "../types";

const modelMeta = {
  model: {
    schema: ModelProviderSchema,
    primaryKeys: ["id"], // The primary key is 'id'
    tableName: "model_providers",
    serializer: (obj: Record<string, unknown>) => {
      // Parse JSON fields
      if (obj.capabilities && typeof obj.capabilities === "string") {
        obj.capabilities = JSON.parse(obj.capabilities as string);
      }
      if (obj.inputModalities && typeof obj.inputModalities === "string") {
        obj.inputModalities = JSON.parse(obj.inputModalities as string);
      }
      if (obj.outputModalities && typeof obj.outputModalities === "string") {
        obj.outputModalities = JSON.parse(obj.outputModalities as string);
      }
      return obj;
    },
    serializerObject: ModelProviderSchema,
  },
  // Path parameter for ID that can contain slashes
  pathParameters: ["id"],
};

// Models are read-only - they're seeded from migrations
export class GetModel extends D1ReadEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = modelMeta;

  async fetch(filters: any) {
    // The ID comes directly from the path parameter
    const id = filters.filters.find((f: any) => f.field === "id")?.value;

    if (!id) {
      return null;
    }

    const obj = await this.getDBBinding()
      .prepare(`SELECT * FROM ${this.meta.model.tableName} WHERE id = ?`)
      .bind(id)
      .first();

    return obj;
  }
}

export class ListModels extends D1ListEndpoint<HandleArgs> {
  //@ts-expect-error
  _meta = modelMeta;
  filterFields = ["provider", "name"];
}
