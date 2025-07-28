# WorkOS SCIM Integration Plan

This document outlines the research and implementation plan for integrating WorkOS for SCIM-based authentication in the `slipstream-agents` worker. This will replace the existing simple bearer token authentication.

## 1. Current Authentication

The current authentication mechanism is implemented in `src/middleware/auth.ts`. It's a basic bearer token authentication that checks against a single, static `API_KEY` stored in the environment. This is not suitable for a multi-tenant environment where different organizations need to connect their directories.

## 2. WorkOS SCIM Authentication

WorkOS provides Directory Sync functionality using the SCIM 2.0 protocol. The authentication method for SCIM is a long-lived bearer token. Each organization with a synced directory will have a unique SCIM endpoint and bearer token provided by WorkOS.

Our worker needs to expose a SCIM endpoint that can accept requests from various identity providers (like Okta, Entra ID, etc.) configured through WorkOS.

## 3. Implementation Plan

### Step 1: Storing SCIM Tokens

SCIM bearer tokens are sensitive and should be treated as secrets. We will use Cloudflare's secrets management for this.

- For each organization, a unique SCIM bearer token will be generated in the WorkOS dashboard.
- This token will be added as a secret to the Cloudflare worker environment. We can use a naming convention like `WORKOS_SCIM_TOKEN_<ORGANIZATION_SLUG>`.
- We will also need to store a mapping from the token to the organization it belongs to. This can be done by convention in the secret name, or by having a separate secret that contains a JSON mapping of tokens to organization IDs. For simplicity, we'll start by iterating over the secrets.

### Step 2: Create a New Authentication Middleware

We will create a new middleware, `workosAuth.ts`, in `src/middleware`. This middleware will replace the existing `bearerAuth` middleware.

The logic for the middleware will be as follows:

1.  **Extract the Bearer Token**: Get the token from the `Authorization` header. If it's not present or doesn't have the "Bearer " prefix, return a 401 Unauthorized error.

2.  **Validate the Token**:
    -   Iterate through the `WORKOS_SCIM_TOKEN_*` secrets in the worker's environment (`c.env`).
    -   Compare the extracted token with each of the stored secret tokens.
    -   If a match is found, extract the organization slug from the secret's name.

3.  **Set Organization Context**:
    -   If a token is validated, set the organization context for downstream handlers. For example:
        ```typescript
        c.set('auth', {
          organizationSlug: 'some-org', // Extracted from the secret name
        });
        ```

4.  **Handle Invalid Tokens**: If the token does not match any of the stored secrets, return a 401 Unauthorized error.

### Step 3: Update Environment and Configuration

- Add the new WorkOS SCIM secrets to `wrangler.toml` for local development (using `.dev.vars`) and to the Cloudflare dashboard for production.
- The `worker-configuration.d.ts` file will need to be updated to reflect the new environment variables for the WorkOS SCIM tokens. We can use a more dynamic type to allow for multiple organization tokens.

### Step 4: Integrate the Middleware

- In `src/index.ts`, replace the `bearerAuth` middleware with the new `workosAuth` middleware on the routes that need to be protected. This will likely be all the SCIM-related endpoints.

## 4. Example Middleware (`src/middleware/workosAuth.ts`)

```typescript
import type { MiddlewareHandler } from "hono";
import { HTTPException } from "hono/http-exception";
import type { AppHonoEnv } from "../types";

export const workosAuth: MiddlewareHandler<AppHonoEnv> = async (c, next) => {
  const authHeader = c.req.header("Authorization");

  if (!authHeader || !authHeader.startsWith("Bearer ")) {
    throw new HTTPException(401, { message: "Unauthorized" });
  }

  const token = authHeader.substring(7);
  let organizationSlug: string | null = null;

  for (const key in c.env) {
    if (key.startsWith("WORKOS_SCIM_TOKEN_")) {
      if (c.env[key] === token) {
        organizationSlug = key.replace("WORKOS_SCIM_TOKEN_", "");
        break;
      }
    }
  }

  if (!organizationSlug) {
    throw new HTTPException(401, { message: "Invalid SCIM token" });
  }

  // Set the organization context
  c.set("auth", {
    organizationSlug: organizationSlug,
  });

  await next();
};
```

## 5. Next Steps

1.  Implement the `workosAuth.ts` middleware.
2.  Update `wrangler.jsonc` and create a `.dev.vars` file for local testing with a dummy token.
3.  Update `worker-configuration.d.ts` to include the new secrets.
4.  Replace the old `bearerAuth` middleware in `src/index.ts`.
5.  Add tests for the new middleware.
