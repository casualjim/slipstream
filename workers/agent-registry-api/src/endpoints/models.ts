import { D1ListEndpoint, D1ReadEndpoint } from "chanfana";
import { z } from "zod";
import type { HandleArgs } from "../types";
import { Modalities, ModelCapabilities, ModelProviderSchema } from "../types";

const jsonArray = z.string().transform((val) => {
  try {
    return JSON.parse(val);
  } catch {
    return [];
  }
});

const modelMeta = {
  fields: z.object({
    id: ModelProviderSchema.shape.id,
    name: ModelProviderSchema.shape.name,
    provider: ModelProviderSchema.shape.provider,
    description: ModelProviderSchema.shape.description,
    contextSize: ModelProviderSchema.shape.contextSize,
    maxTokens: ModelProviderSchema.shape.maxTokens,
    temperature: ModelProviderSchema.shape.temperature,
    topP: ModelProviderSchema.shape.topP,
    frequencyPenalty: ModelProviderSchema.shape.frequencyPenalty,
    presencePenalty: ModelProviderSchema.shape.presencePenalty,
    capabilities: z.array(z.nativeEnum(ModelCapabilities)),
    inputModalities: z.array(z.nativeEnum(Modalities)),
    outputModalities: z.array(z.nativeEnum(Modalities)),
    dialect: ModelProviderSchema.shape.dialect,
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
/**
 * ## Get Model
 *
 * Retrieves a specific model by its ID.
 * Models are read-only and seeded from migrations, so this endpoint provides
 * a way to look up their details.
 */
export class GetModel extends D1ReadEndpoint<HandleArgs> {
  // @ts-expect-error - chanfana has poor type definitions
_meta = {
    summary: "Get a specific Model",
    description: "Retrieves a single model by its ID from the registry",
    ...modelMeta,
  };

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

/**
 * ## List Models
 *
 * Retrieves a list of all available models.
 * This endpoint supports filtering by provider and name, searching by name and
 * description, and ordering. Models are read-only and seeded from migrations.
 */
export class ListModels extends D1ListEndpoint<HandleArgs> {
  // @ts-expect-error - chanfana has poor type definitions
_meta = {
    summary: "List all Models",
    description: "Retrieves a list of all models in the registry",
    ...modelMeta,
  };
  // @ts-ignore - chanfana has poor type definitions
  filterFields = ["provider", "name"];
  // @ts-ignore - chanfana has poor type definitions
  searchFields = ["name", "description"];
  // @ts-ignore - chanfana has poor type definitions
  orderByFields = ["name", "provider", "createdAt"];
  defaultOrderBy = "name";
}
