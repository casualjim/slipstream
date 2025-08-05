import { z as zOpenApi } from "@hono/zod-openapi";
import { z } from "zod";

/**
 * Runtime types
 */
export type ApiError = {
  success: false;
  errors: Array<{ message: string; code?: string; field?: string }>;
};
export type ApiListMeta = {
  result_info: {
    count: number;
    page?: number;
    per_page?: number;
    total_count?: number;
  };
};
export type ApiSuccess<T> = { success: true; result: T } & Partial<ApiListMeta>;
export type ApiResponse<T> = ApiSuccess<T> | ApiError;

/**
 * Runtime helpers
 */
export function success<T>(result: T): ApiSuccess<T> {
  return { success: true, result };
}
export function failure(message: string, code?: string, field?: string): ApiError {
  return { success: false, errors: [{ message, code, field }] };
}

/**
 * Zod (runtime) envelopes
 */
export const ErrorItemSchema = z.object({
  message: z.string(),
  code: z.string().optional(),
  field: z.string().optional(),
});
export const ErrorEnvelopeSchema = z.object({
  success: z.literal(false),
  errors: z.array(ErrorItemSchema),
});

/**
 * OpenAPI schemas and helpers
 */
export const ErrorItemOpenApi = zOpenApi.object({
  message: zOpenApi.string(),
  code: zOpenApi.string().optional(),
  field: zOpenApi.string().optional(),
});

export const ErrorEnvelopeOpenApi = zOpenApi.object({
  success: zOpenApi.literal(false),
  errors: zOpenApi.array(ErrorItemOpenApi),
});

export const ResultInfoOpenApi = zOpenApi
  .object({
    count: zOpenApi.number().int().nonnegative(),
    page: zOpenApi.number().int().nonnegative().optional(),
    per_page: zOpenApi.number().int().nonnegative().optional(),
    total_count: zOpenApi.number().int().nonnegative().optional(),
  })
  .optional();

/**
 * Wrap a payload schema into the success envelope for OpenAPI
 */
export function SuccessEnvelopeOpenApi<T extends z.ZodTypeAny>(payload: T) {
  return zOpenApi.object({
    success: zOpenApi.literal(true),
    result: payload,
  });
}

/**
 * Variant for list payloads that add optional result_info
 */
export function SuccessListEnvelopeOpenApi<T extends z.ZodTypeAny>(payload: T) {
  return zOpenApi.object({
    success: zOpenApi.literal(true),
    result: payload,
    result_info: ResultInfoOpenApi,
  });
}
