import { z } from "zod";
import { generateSlug } from "../lib/utils";
import { ProjectSchema } from "../types";

// Schemas for CRUD operations
export const CreateProjectSchema = ProjectSchema.pick({
  name: true,
  slug: true,
  description: true,
  organization: true,
}).extend({
  slug: ProjectSchema.shape.slug.optional(),
});

export const UpdateProjectSchema = ProjectSchema.pick({
  name: true,
  description: true,
  organization: true,
}).partial();

export class ProjectService {
  constructor(private db: D1Database) {}

  async create(data: z.infer<typeof CreateProjectSchema>) {
    const slug = data.slug ?? generateSlug(data.name);
    const now = new Date().toISOString();
    const project = {
      ...data,
      slug,
      createdAt: now,
      updatedAt: now,
    };

    const result = await this.db
      .prepare(
        "INSERT INTO projects (slug, name, description, organization, createdAt, updatedAt) VALUES (?, ?, ?, ?, ?, ?) RETURNING *",
      )
      .bind(
        project.slug,
        project.name,
        project.description ?? null,
        project.organization,
        project.createdAt,
        project.updatedAt,
      )
      .first<z.infer<typeof ProjectSchema>>();

    if (!result) {
      throw new Error("Unable to create project");
    }
    return result;
  }

  async get(slug: string) {
    const result = await this.db
      .prepare("SELECT * FROM projects WHERE slug = ?")
      .bind(slug)
      .first<z.infer<typeof ProjectSchema>>();
    return result;
  }

  async update(slug: string, patch: z.infer<typeof UpdateProjectSchema>) {
    const updatedAt = new Date().toISOString();

    const fields: string[] = [];
    const values: any[] = [];

    if (patch.name !== undefined) {
      fields.push("name = ?");
      values.push(patch.name);
    }
    if (patch.description !== undefined) {
      fields.push("description = ?");
      values.push(patch.description);
    }
    if (patch.organization !== undefined) {
      fields.push("organization = ?");
      values.push(patch.organization);
    }
    fields.push("updatedAt = ?");
    values.push(updatedAt);
    values.push(slug); // for WHERE clause

    const sql = `UPDATE projects SET ${fields.join(", ")} WHERE slug = ? RETURNING *`;

    const result = await this.db.prepare(sql).bind(...values).first<z.infer<typeof ProjectSchema>>();
    return result;
  }

  async delete(slug: string) {
    const result = await this.db.prepare("DELETE FROM projects WHERE slug = ?").bind(slug).run();
    return result.success;
  }

  async list(options: {
    page?: number;
    per_page?: number;
    search?: string;
    name?: string;
    slug?: string;
    organization?: string;
    orderBy?: "name" | "createdAt" | "updatedAt";
  }) {
    const per = options.per_page ? Math.max(1, options.per_page) : 20;
    const page = Math.max(1, options.page ?? 1);
    const offset = (page - 1) * per;

    const where: string[] = [];
    const bind: unknown[] = [];

    if (options.organization) {
      where.push("organization = ?");
      bind.push(options.organization);
    }
    if (options.name) {
      where.push(`LOWER(name) LIKE LOWER(?)`);
      bind.push(`%${options.name}%`);
    }
    if (options.slug) {
      where.push(`slug = ?`);
      bind.push(options.slug);
    }
    if (options.search) {
      where.push(`(name LIKE ? OR description LIKE ?)`);
      bind.push(`%${options.search}%`, `%${options.search}%`);
    }

    const clauses = where.length ? `WHERE ${where.join(" AND ")}` : "";
    const orderBy = options.orderBy ?? "name";
    const order = "ASC";
    const limitClause = `LIMIT ${per} OFFSET ${offset}`;

    const countRow = await this.db
      .prepare(`SELECT COUNT(*) as total FROM projects ${clauses}`)
      .bind(...bind)
      .first<{ total: number }>();
    const total_count = countRow?.total ?? 0;

    const sql = `
      SELECT * FROM projects
      ${clauses}
      ORDER BY ${orderBy} ${order}
      ${limitClause}
    `;
    const res = await this.db.prepare(sql).bind(...bind).all<z.infer<typeof ProjectSchema>>();
    const items = res.results ?? [];

    return {
      items,
      result_info: {
        count: items.length,
        page,
        per_page: per,
        total_count,
      },
    };
  }

  async checkSlugExists(slug: string): Promise<boolean> {
    const result = await this.db.prepare("SELECT 1 FROM projects WHERE slug = ? LIMIT 1").bind(slug).first();
    return !!result;
  }

  async belongsToOrganization(projectSlug: string, organization: string): Promise<boolean> {
    const result = await this.db
      .prepare("SELECT 1 FROM projects WHERE slug = ? AND organization = ? LIMIT 1")
      .bind(projectSlug, organization)
      .first();
    return !!result;
  }
}
