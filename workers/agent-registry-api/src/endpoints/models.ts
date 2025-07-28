import { D1ListEndpoint, D1ReadEndpoint } from "chanfana";
import { z } from "zod";
import type { HandleArgs } from "../types";
import { APIDialect, Modalities, ModelCapabilities, ModelProviderSchema, Provider } from "../types";

const jsonArray = z.string().transform((val) => {
  try {
    return JSON.parse(val);
  } catch {
    return [];
  }
});

const modelMeta = {
  fields: z.object({
    id: z.string(),
    name: z.string(),
    provider: z.nativeEnum(Provider),
    description: z.string().nullish(),
    contextSize: z.number(),
    maxTokens: z.number().nullish(),
    temperature: z.number().nullish(),
    topP: z.number().nullish(),
    frequencyPenalty: z.number().nullish(),
    presencePenalty: z.number().nullish(),
    capabilities: z.array(z.nativeEnum(ModelCapabilities)),
    inputModalities: z.array(z.nativeEnum(Modalities)),
    outputModalities: z.array(z.nativeEnum(Modalities)),
    dialect: z.nativeEnum(APIDialect).nullish(),
  }),
  model: {
    schema: ModelProviderSchema,
    primaryKeys: ["id"],
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
  // @ts-ignore - chanfana has poor type definitions
  filterFields = ["provider", "name"];
  // @ts-ignore - chanfana has poor type definitions
  searchFields = ["name", "description"];
  // @ts-ignore - chanfana has poor type definitions
  orderByFields = ["name", "provider", "createdAt"];
  defaultOrderBy = "name";
}
