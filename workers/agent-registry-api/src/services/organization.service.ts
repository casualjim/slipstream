import { z } from "zod";
import { generateSlug } from "../lib/utils";

// Minimal organization schema (mirroring style used by agents/tools with Zod parsing on serialization path).
// If a dedicated schema exists later, this can be replaced to import it.
export const OrganizationSchema = z.object({
  slug: z.string().min(1),
  name: z.string().min(1),
  description: z.string().optional(),
  createdAt: z.string().readonly(),
  updatedAt: z.string().readonly(),
});

export type Organization = z.infer<typeof OrganizationSchema>;

export const CreateOrganizationSchema = z.object({
  slug: z.string().min(1).optional(),
  name: z.string().min(1),
  description: z.string().optional(),
});
export type CreateOrganizationInput = z.infer<typeof CreateOrganizationSchema>;

export const UpdateOrganizationSchema = z.object({
  name: z.string().min(1).optional(),
  description: z.string().optional(),
});
export type UpdateOrganizationInput = z.infer<typeof UpdateOrganizationSchema>;

// DB row shape
type OrganizationDbRow = {
  slug: string;
  name: string;
  description: string | null;
  createdAt: string;
  updatedAt: string;
};

function nowIso(): string {
  return new Date().toISOString();
}

function serializeRow(row: OrganizationDbRow): Organization {
  const out = {
    slug: row.slug,
    name: row.name,
    description: row.description ?? undefined,
    createdAt: row.createdAt,
    updatedAt: row.updatedAt,
  };
  return OrganizationSchema.parse(out);
}

/**
 * OrganizationService — mirrors AgentService/ToolService patterns:
 * - slug generation from name
 * - fail-fast DB serialization
 * - list with pagination and simple filters
 */
export class OrganizationService {
  constructor(private readonly db: D1Database) {}

  async create(input: CreateOrganizationInput) {
    const slug = input.slug ?? generateSlug(input.name);
    const createdAt = nowIso();
    const updatedAt = createdAt;

    await this.db
      .prepare(
        `INSERT INTO organizations (slug, name, description, createdAt, updatedAt)
         VALUES (?, ?, ?, ?, ?)`,
      )
      .bind(slug, input.name, input.description ?? null, createdAt, updatedAt)
      .run();

    const row = await this.db
      .prepare(
        `SELECT slug, name, description, createdAt, updatedAt
         FROM organizations WHERE slug = ?`,
      )
      .bind(slug)
      .first<OrganizationDbRow>();

    if (!row) throw new Error("Failed to read created organization");
    return serializeRow(row);
  }

  async get(slug: string) {
    const row = await this.db
      .prepare(
        `SELECT slug, name, description, createdAt, updatedAt
         FROM organizations WHERE slug = ?`,
      )
      .bind(slug)
      .first<OrganizationDbRow>();
    if (!row) return null;
    return serializeRow(row);
  }

  async update(slug: string, patch: UpdateOrganizationInput) {
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
    sets.push("updatedAt = ?");
    values.push(nowIso());

    if (sets.length === 1) { // Only updatedAt was added
      return this.get(slug);
    }

    const sql = `UPDATE organizations SET ${sets.join(", ")} WHERE slug = ?`;
    values.push(slug);

    await this.db.batch([
      this.db.prepare(sql).bind(...values),
    ]);
    return this.get(slug);
  }

  async delete(slug: string) {
    const res = await this.db.batch([
      this.db.prepare(`SELECT 1 FROM projects WHERE organization = ? LIMIT 1`).bind(slug),
      this.db.prepare(`DELETE FROM organizations WHERE slug = ?`).bind(slug),
    ]);

    const selectRes = res[0];
    const hasProjects = selectRes.results && selectRes.results.length > 0;
    if (hasProjects) {
      throw new Error("Cannot delete organization with existing projects");
    }

    const deleteRes = res[1];
    const changes = deleteRes.meta?.changes;
    if (typeof changes === "number") return changes > 0;

    const still = await this.db.prepare(`SELECT 1 FROM organizations WHERE slug = ?`).bind(slug).first();
    return !still;
  }

  async list(filters?: {
    name?: string;
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
    if (filters?.search) {
      where.push("(name LIKE ? OR slug LIKE ?)");
      const pattern = `%${filters.search}%`;
      bind.push(pattern, pattern);
    }

    const clauses = where.length ? `WHERE ${where.join(" AND ")}` : "";
    const order = filters?.orderBy ?? "createdAt";

    const per = filters?.per_page ? Math.max(1, filters.per_page) : undefined;
    const page = Math.max(1, filters?.page ?? 1);
    const offset = per ? (page - 1) * per : 0;
    const limitClause = per ? ` LIMIT ${per} OFFSET ${offset}` : "";

    const countSql = `
      SELECT COUNT(*) as total
      FROM organizations
      ${clauses}
    `;
    const countRow = await this.db
      .prepare(countSql)
      .bind(...bind)
      .first<{ total: number }>();
    const total_count = countRow?.total ?? 0;

    const sql = `
      SELECT slug, name, description, createdAt, updatedAt
      FROM organizations
      ${clauses}
      ORDER BY ${order} ASC
      ${limitClause}
    `;
    const res = await this.db
      .prepare(sql)
      .bind(...bind)
      .all<OrganizationDbRow>();
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
}
