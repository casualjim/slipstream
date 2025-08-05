/**
* Cloudflare Workers + Hono helpers for typed bindings and D1 access.
*
* Rationale:
* - Cloudflare bindings are available per request via c.env (app.fetch(request, env, ctx)).
* - Hono exposes those bindings on Context.env; this is the canonical access point.
* - We add light, typed helpers to avoid ad-hoc casting while preserving request scoping.
* - No globals, no app wiring changes, no assumptions about binding names beyond a default.
*
* This file is self-contained and does not modify any other modules.
*/
import type { AppContext } from "../types";

/**
* Optional healthcheck for D1 connectivity using a lightweight PRAGMA.
*/
export async function d1Healthcheck(c: AppContext): Promise<boolean> {
  try {
    const db = c.env.DB as D1Database;
    await db.prepare("PRAGMA user_version;").first();
    return true;
  } catch {
    return false;
  }
}
