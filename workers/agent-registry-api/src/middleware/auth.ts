import type { MiddlewareHandler } from "hono";
import { HTTPException } from "hono/http-exception";
import type { AppHonoEnv } from "../types";

export const bearerAuth: MiddlewareHandler<AppHonoEnv> = async (c, next) => {
  const auth = c.req.header("Authorization");

  if (!auth?.startsWith("Bearer ")) {
    throw new HTTPException(401, { message: "Unauthorized" });
  }

  const token = auth.substring(7);
  if (token !== c.env.API_KEY) {
    throw new HTTPException(401, { message: "Invalid API key" });
  }

  // Set test user context
  c.set("auth", {
    userId: "01983d84-2e1d-747d-8e58-fdeb3f5d241d",
    organizations: ["wagyu"],
  });

  await next();
};
