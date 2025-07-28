import { MiddlewareHandler } from "hono";
import { HTTPException } from "hono/http-exception";
import type { AppHonoEnv } from "../types";

// Middleware to check if user has access to the organization
export const checkOrgAccess: MiddlewareHandler<AppHonoEnv> = async (c, next) => {
  const auth = c.get("auth");
  const orgId = c.req.param("id");

  if (!orgId || auth.organizations.includes(orgId)) {
    throw new HTTPException(403, { message: "Access denied to organization" });
  }

  await next();
};

// Middleware to filter list results by user's organizations
export const filterOrgList: MiddlewareHandler<AppHonoEnv> = async (c, next) => {
  // This would need to be implemented differently
  // For now, we'll handle this in the service layer or with a custom endpoint
  await next();
};
