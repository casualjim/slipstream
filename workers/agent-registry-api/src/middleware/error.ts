import type { ErrorHandler } from "hono";
import { HTTPException } from "hono/http-exception";
import type { AppHonoEnv } from "../types";

/**
 * Global error handler for Hono app.
 * Normalizes errors into a consistent JSON envelope and status code.
 */
export const onError: ErrorHandler<AppHonoEnv> = (err, c) => {
  // Hono HTTPException exposes status and message
  if (err instanceof HTTPException) {
    return c.json(
      {
        success: false,
        errors: [{ code: err.status, message: err.message }],
      },
      err.status,
    );
  }

  // Unknown/unhandled error
  console.error("Unhandled error:", err);
  return c.json(
    {
      success: false,
      errors: [{ code: 7000, message: "Internal Server Error" }],
    },
    500,
  );
};
