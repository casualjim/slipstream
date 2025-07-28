# WorkOS Integration Plan for User Management and API Key Authentication

This document outlines the research and implementation plan for integrating WorkOS for user management and a custom API key authentication system in the `slipstream-agents` worker. This will replace the existing simple bearer token authentication.

## 1. Goal

The objective is to replace the current authentication middleware with a system where:
-   Users are managed (created, read, updated, deleted) through WorkOS.
-   Users can generate and use API keys to authenticate with the service.

WorkOS's User Management and AuthKit products are designed for interactive user sign-in flows (e.g., email/password, SSO) and do not provide a built-in solution for end-user API key generation and validation. Therefore, we will implement a hybrid approach: WorkOS for user lifecycle management and a custom solution for API keys.

## 2. Implementation Plan

### Step 1: User Management with WorkOS

We will use the [WorkOS User Management API](https://workos.com/docs/reference/user-management) as the source of truth for user and organization data.

-   **Dependencies**: We will need to add the `@workos-inc/node` SDK to the project.
-   **Configuration**: The WorkOS API key and Client ID will be stored as secrets in the Cloudflare worker environment (e.g., `WORKOS_API_KEY`, `WORKOS_CLIENT_ID`).
-   **User Provisioning**: We will need to decide on a strategy for provisioning users in WorkOS. This could be done through an admin interface, an invitation flow, or by allowing users to sign up. For the initial implementation, we can assume users are provisioned manually or through a separate admin process.

### Step 2: Custom API Key Management

We will build a system for generating, storing, and validating API keys, backed by our D1 database.

-   **Database Schema**: A new table, `api_keys`, will be created in the D1 database.
    ```sql
    CREATE TABLE api_keys (
      id TEXT PRIMARY KEY,
      hashed_key TEXT NOT NULL UNIQUE,
      user_id TEXT NOT NULL,
      organization_id TEXT NOT NULL,
      created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
      expires_at TIMESTAMP,
      last_used_at TIMESTAMP,
      name TEXT
    );
    ```
-   **API Key Generation**:
    -   We will create new API endpoints (e.g., `/api/keys`) for authenticated users to manage their API keys.
    -   When a user requests a new key, we will generate a cryptographically secure random string.
    -   The key will be hashed using a strong, one-way hashing algorithm (e.g., SHA-256) before being stored in the `hashed_key` column.
    -   The unhashed key will be returned to the user **only once**. They will be responsible for storing it securely.
-   **API Key Revocation**: Endpoints will be provided for users to delete (revoke) their API keys.

### Step 3: Authentication Middleware

A new middleware, `apiKeyAuth.ts`, will be created to handle API key authentication.

1.  **Extract API Key**: The middleware will extract the key from the `Authorization: Bearer <key>` header.

2.  **Validate Key**:
    -   The provided key will be hashed.
    -   The middleware will query the `api_keys` table in the D1 database for the `hashed_key`.
    -   It will check if the key exists and has not expired.

3.  **Set User Context**:
    -   If the key is valid, the `user_id` and `organization_id` will be retrieved from the database.
    -   This information will be used to set the authentication context for the request, for example:
        ```typescript
        c.set('auth', {
          userId: 'user_123',
          organizationId: 'org_abc',
        });
        ```
    -   The `last_used_at` timestamp for the key will be updated.

4.  **Handle Invalid Keys**: If the key is not found or is invalid, a 401 Unauthorized error will be returned.

### Step 4: Integration

-   The new `apiKeyAuth` middleware will be applied to the relevant routes in `src/index.ts`, replacing the old `bearerAuth` middleware.
-   The WorkOS SDK will be initialized in a shared context or on-demand in the service layer to interact with the WorkOS API for user management tasks.

## 5. Example Middleware (`src/middleware/apiKeyAuth.ts`)

```typescript
import type { MiddlewareHandler } from "hono";
import { HTTPException } from "hono/http-exception";
import type { AppHonoEnv } from "../types";

// This is a simplified example. In a real implementation, you would use a
// library like `crypto` to perform the hashing.
async function hashApiKey(key: string): Promise<string> {
  const encoder = new TextEncoder();
  const data = encoder.encode(key);
  const hash = await crypto.subtle.digest('SHA-256', data);
  return Array.from(new Uint8Array(hash)).map(b => b.toString(16).padStart(2, '0')).join('');
}

export const apiKeyAuth: MiddlewareHandler<AppHonoEnv> = async (c, next) => {
  const authHeader = c.req.header("Authorization");

  if (!authHeader || !authHeader.startsWith("Bearer ")) {
    throw new HTTPException(401, { message: "Unauthorized" });
  }

  const token = authHeader.substring(7);
  const hashedToken = await hashApiKey(token);

  const { results } = await c.env.DB.prepare(
    "SELECT user_id, organization_id FROM api_keys WHERE hashed_key = ? AND (expires_at IS NULL OR expires_at > datetime('now'))"
  ).bind(hashedToken).all();

  if (!results || results.length === 0) {
    throw new HTTPException(401, { message: "Invalid API key" });
  }

  const apiKey = results[0] as { user_id: string; organization_id: string };

  // Update the last_used_at timestamp (fire and forget)
  c.executionCtx.waitUntil(
    c.env.DB.prepare("UPDATE api_keys SET last_used_at = datetime('now') WHERE hashed_key = ?")
      .bind(hashedToken)
      .run()
  );

  c.set("auth", {
    userId: apiKey.user_id,
    organizationId: apiKey.organization_id,
  });

  await next();
};
```

## 6. Next Steps

1.  Add the `@workos-inc/node` dependency.
2.  Add WorkOS secrets (`WORKOS_API_KEY`, `WORKOS_CLIENT_ID`) to the environment.
3.  Create the `api_keys` table migration for D1.
4.  Implement the API key management endpoints.
5.  Implement the `apiKeyAuth.ts` middleware.
6.  Integrate the new middleware and WorkOS SDK into the application.
7.  Write tests for the new functionality.
