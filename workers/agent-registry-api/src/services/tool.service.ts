import { parse, rcompare } from "semver";
import { generateSlug } from "../lib/utils";
import { type CreateToolInput, ToolSchema, type UpdateToolInput } from "../schemas/tool";

/**
 * Runtime row shape coming from D1 for the tools table.
 * Arguments are stored as TEXT (JSON string), dates are DATETIME strings.
 */
type ToolDbRow = {
  slug: string;
  version: string;
  provider: "Client" | "Local" | "MCP" | "Restate";
  name: string;
  description: string | null;
  arguments: string | null; // JSON string
  createdAt: string; // DATETIME
  updatedAt: string; // DATETIME
  // Optional provider-specific configs persisted alongside arguments (if schema extended later)
  restate?: unknown;
  mcp?: unknown;
};

// Utilities
function nowIso(): string {
  return new Date().toISOString();
}

function serializeRow(row: ToolDbRow) {
  // Parse JSON columns with fail-fast semantics
  let argsObj: Record<string, unknown> | undefined = undefined;
  if (row.arguments && row.arguments.length) {
    // Propagate parsing errors to surface bad DB contents
    const parsed = JSON.parse(row.arguments);
    if (parsed !== null && typeof parsed !== "object") {
      throw new Error("Invalid arguments JSON: expected object or null");
    }
    argsObj = parsed ?? undefined;
  }

  // D1 returns NULL for absent columns -> treat as undefined for optional objects.
  const rawRestate = row.restate;
  const rawMcp = row.mcp;

  const parseOptionalJson = (val: unknown, field: "restate" | "mcp"): unknown => {
    if (val == null) return undefined;
    if (typeof val === "string") {
      const s = val.trim();
      if (s === "") return undefined;
      const parsed = JSON.parse(s); // will throw on invalid JSON
      if (parsed !== null && typeof parsed !== "object") {
        throw new Error(`Invalid ${field} JSON: expected object or null`);
      }
      return parsed ?? undefined;
    }
    if (typeof val === "object") return val;
    throw new Error(`Invalid ${field} value type: ${typeof val}`);
  };

  const restate = parseOptionalJson(rawRestate, "restate");
  const mcp = parseOptionalJson(rawMcp, "mcp");

  const out = {
    slug: row.slug,
    version: row.version,
    provider: row.provider,
    name: row.name,
    description: row.description ?? undefined,
    arguments: argsObj,
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
    restate,
    mcp,
  };

  // Validate row strictly against ToolSchema shape.
  return ToolSchema.parse(out);
}

/**
 * Typed ToolService using D1Database
 */
export class ToolService {
  constructor(private readonly db: D1Database) {}

  // Create a tool. Auto-generate slug if missing based on name.
  async create(input: CreateToolInput) {
    // Normalize slug if not provided using shared utils (covers special chars mapping)
    const slug = input.slug ?? generateSlug(input.name);

    const createdAt = nowIso();
    const updatedAt = createdAt;

    // Persist arguments as JSON string or null
    const argsJson = typeof input.arguments !== "undefined" ? JSON.stringify(input.arguments ?? null) : null;
    // Persist provider-specific configs when present as JSON TEXT columns
    const mcpJson =
      typeof input.mcp !== "undefined" ? JSON.stringify(input.mcp ?? null) : null;
    const restateJson =
      typeof input.restate !== "undefined" ? JSON.stringify(input.restate ?? null) : null;

    // Insert (respecting composite PK)
    await this.db
      .prepare(
        `INSERT INTO tools (slug, version, provider, name, description, arguments, mcp, restate, createdAt, updatedAt)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
      )
      .bind(
        slug,
        input.version,
        input.provider,
        input.name,
        input.description ?? null,
        argsJson,
        mcpJson,
        restateJson,
        createdAt,
        updatedAt,
      )
      .run();

    // Read back the row and validate with full ToolSchema (including provider-specific rules)
    const row = await this.db
      .prepare(
        `SELECT slug, version, provider, name, description, arguments, createdAt, updatedAt, mcp, restate
         FROM tools WHERE slug = ? AND version = ? AND provider = ?`,
      )
      .bind(slug, input.version, input.provider)
      .first<ToolDbRow>();

    if (!row) {
      throw new Error("Failed to read created tool");
    }

    // Strict validation: ensure DB row conforms to ToolSchema
    const validated = ToolSchema.parse(serializeRow(row));
    return validated;
  }

  async get(provider: ToolDbRow["provider"], slug: string, version: string) {
    const row = await this.db
      .prepare(
        `SELECT slug, version, provider, name, description, arguments, createdAt, updatedAt, mcp, restate
         FROM tools WHERE provider = ? AND slug = ? AND version = ?`,
      )
      .bind(provider, slug, version)
      .first<ToolDbRow>();
    if (!row) return null;

    // Validate row; do not swallow errors here. If invalid data is present in DB,
    // let the caller decide how to handle thrown errors.
    const tool = serializeRow(row);
    // Do not convert provider-specific missing configs to null here; rely on schema truth.
    return tool;
  }

  async update(provider: ToolDbRow["provider"], slug: string, version: string, patch: UpdateToolInput) {
    // Build dynamic SET clause based on provided fields
    const sets: string[] = [];
    const values: unknown[] = [];

    if (typeof patch.name !== "undefined") {
      sets.push("name = ?");
      values.push(patch.name);
    }
    if (typeof patch.description !== "undefined") {
      sets.push("description = ?");
      values.push(patch.description ?? null);
    }
    if (typeof patch.arguments !== "undefined") {
      sets.push("arguments = ?");
      values.push(JSON.stringify(patch.arguments ?? null));
    }
    if (typeof patch.restate !== "undefined") {
      sets.push("restate = ?");
      values.push(JSON.stringify(patch.restate ?? null));
    }
    if (typeof patch.mcp !== "undefined") {
      sets.push("mcp = ?");
      values.push(JSON.stringify(patch.mcp ?? null));
    }

    // Always update updatedAt
    sets.push("updatedAt = ?");
    values.push(nowIso());

    if (sets.length === 0) {
      // Nothing to update; return current record
      return this.get(provider, slug, version);
    }

    const sql = `UPDATE tools SET ${sets.join(", ")} WHERE provider = ? AND slug = ? AND version = ?`;
    values.push(provider, slug, version);

    const res = await this.db
      .prepare(sql)
      .bind(...values)
      .run();
    // If no changes, still read back to confirm existence; allow validation errors to propagate
    const after = await this.get(provider, slug, version);
    return after; // null => not found
  }

  async delete(provider: ToolDbRow["provider"], slug: string, version: string) {
    const res = await this.db
      .prepare(`DELETE FROM tools WHERE provider = ? AND slug = ? AND version = ?`)
      .bind(provider, slug, version)
      .run();
    // Prefer definitive change count if available; fall back to existence check
    const changes = res.meta?.changes;
    if (typeof changes === "number") {
      return changes > 0;
    }
    const still = await this.db
      .prepare(
        `SELECT 1 FROM tools WHERE provider = ? AND slug = ? AND version = ?`,
      )
      .bind(provider, slug, version)
      .first();
    return !still;
  }

  /**
   * Simple list with optional basic filters:
   * - name?: string (exact match)
   * - provider?: Provider
   * - search?: string (LIKE on name/slug)
   * - orderBy?: 'name' | 'createdAt' | 'updatedAt' (default createdAt)
   * - page?: number (1-based)
   * - per_page?: number
   */
  async list(filters?: {
    name?: string;
    provider?: ToolDbRow["provider"];
    search?: string;
    orderBy?: "name" | "createdAt" | "updatedAt";
    page?: number;
    per_page?: number;
  }) {
    const where: string[] = [];
    const bind: unknown[] = [];

    if (filters?.name) {
      where.push("name = ?");
      bind.push(filters.name);
    }
    if (filters?.provider) {
      where.push("provider = ?");
      bind.push(filters.provider);
    }
    if (filters?.search) {
      where.push("(name LIKE ? OR slug LIKE ?)");
      const pattern = `%${filters.search}%`;
      bind.push(pattern, pattern);
    }

    const order = filters?.orderBy ?? "createdAt";
    const clauses = where.length ? `WHERE ${where.join(" AND ")}` : "";

    const per = filters?.per_page ? Math.max(1, filters.per_page) : undefined;
    const page = Math.max(1, filters?.page ?? 1);
    const offset = per ? (page - 1) * per : 0;
    const limitClause = per ? ` LIMIT ${per} OFFSET ${offset}` : "";

    // total count for pagination metadata (unpaged)
    const countSql = `
      SELECT COUNT(*) as total
      FROM tools
      ${clauses}
    `;
    const countRow = await this.db.prepare(countSql).bind(...bind).first<{ total: number }>();
    const total_count = countRow?.total ?? 0;

    const sql = `
      SELECT slug, version, provider, name, description, arguments, createdAt, updatedAt, mcp, restate
      FROM tools
      ${clauses}
      ORDER BY ${order} ASC
      ${limitClause}
    `;

    const res = await this.db.prepare(sql).bind(...bind).all<ToolDbRow>();
    // Do not swallow individual item errors; this will throw if any row is invalid.
    const items = res.results.map((r) => serializeRow(r));

    return {
      items,
      result_info: {
        count: items.length,
        page: per ? page : undefined,
        per_page: per ?? undefined,
        total_count,
      },
    };
  }

  /**
   * Latest by slug for a given provider:
   * - Prefer highest semantic version; if pre-release exists, stable wins over pre-release.
   * - If only pre-releases exist, return the highest pre-release.
   * - Build metadata (+build) does not affect precedence; any of equal base versions is acceptable.
   */
  async getLatestBySlug(provider: ToolDbRow["provider"], slug: string) {
    // Fetch all versions for provider+slug
    const res = await this.db
      .prepare(
        `SELECT slug, version, provider, name, description, arguments, createdAt, updatedAt, mcp, restate
         FROM tools WHERE provider = ? AND slug = ?`,
      )
      .bind(provider, slug)
      .all<ToolDbRow>();
    const rows = res.results ?? [];
    if (!rows.length) return null;

    // Split into stable and prerelease using semver.parse with includePrerelease
    const stable: ToolDbRow[] = [];
    const prerelease: ToolDbRow[] = [];
    for (const row of rows) {
      const v = parse(row.version, { includePrerelease: true });
      if (!v) continue;
      if (v.prerelease.length === 0) {
        stable.push(row);
      } else {
        prerelease.push(row);
      }
    }

    const pickLatest = (arr: ToolDbRow[]): ToolDbRow | null => {
      if (arr.length === 0) return null;
      const sorted = [...arr].sort((a, b) => rcompare(a.version, b.version)); // descending
      return sorted[0] ?? null;
    };

    const chosen = pickLatest(stable) ?? pickLatest(prerelease);
    if (!chosen) return null;

    // Read back the full chosen record via get() to ensure consistent normalization/validation
    const tool = await this.get(chosen.provider, chosen.slug, chosen.version);
    return tool;
  }
}
