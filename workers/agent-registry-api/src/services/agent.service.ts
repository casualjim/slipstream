import { AgentService as SharedAgentService } from "../lib/services/agents";
import { generateSlug, isValidSlug } from "../lib/utils";
import { type Agent, AgentSchema, type CreateAgentInput, type UpdateAgentInput } from "../schemas/agent";

/**
 * Thin adapter delegating to shared lib/services/agents for all DB logic and JSON handling.
 * Strictly typed: no any.
 */
export class AgentService {
  private readonly shared: SharedAgentService;

  constructor(private readonly db: D1Database) {
    this.shared = new SharedAgentService(db);
  }

  async create(input: CreateAgentInput, userId: string): Promise<Agent> {
    const slug = input.slug ?? generateSlug(input.name);
    if (!isValidSlug(slug)) throw new Error("Invalid slug");

    // Normalize optional fields to avoid undefined being sent to D1
    const nowIso = new Date().toISOString();
    const payload = {
      slug,
      name: input.name,
      version: input.version,
      model: input.model,
      instructions: input.instructions,
      // description should be either string or null (not undefined)
      description: input.description ?? null,
      organization: input.organization,
      project: input.project,
      // createdBy/updatedBy must reference valid user IDs
      createdBy: userId,
      updatedBy: userId,
      createdAt: nowIso,
      updatedAt: nowIso,
      // availableTools as string[] or undefined
      availableTools: input.availableTools && input.availableTools.length > 0 ? input.availableTools : undefined,
    };

    return await this.shared.create(payload);
  }

  async get(slug: string, version: string): Promise<Agent | null> {
    const item = await this.shared.getBySlugVersion(slug, version);
    return item ? AgentSchema.parse(item) : null;
  }

  async update(slug: string, version: string, patch: UpdateAgentInput, userId: string): Promise<Agent | null> {
    // Add updatedBy and updatedAt to the patch
    const patchWithMetadata = {
      ...patch,
      updatedBy: userId,
      updatedAt: new Date().toISOString(),
    };
    const updated = await this.shared.update(slug, version, patchWithMetadata as Partial<Agent>);
    return updated.result ? AgentSchema.parse(updated.result as Agent) : null;
  }

  async delete(slug: string, version: string): Promise<boolean> {
    const res = await this.shared.delete(slug, version);
    return res.success === true;
  }

  async list(filters?: {
    name?: string;
    search?: string;
    orderBy?: "name" | "createdAt" | "updatedAt";
    page?: number;
    per_page?: number;
    organization?: string;
    project?: string;
  }): Promise<{
    items: Agent[];
    result_info: { count: number; page?: number; per_page?: number; total_count: number };
  }> {
    // Retain existing SQL but strictly type outputs through AgentSchema
    const where: string[] = [];
    const bind: (string | number)[] = [];

    if (filters?.name) {
      where.push("name = ?");
      bind.push(filters.name);
    }
    if (filters?.organization) {
      where.push("organization = ?");
      bind.push(filters.organization);
    }
    if (filters?.project) {
      where.push("project = ?");
      bind.push(filters.project);
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
      FROM agents
      ${clauses}
    `;
    const countRow = await this.db
      .prepare(countSql)
      .bind(...bind)
      .first<{ total: number }>();
    const total_count = (countRow?.total ?? 0) as number;

    type Row = {
      slug: string;
      version: string;
      name: string;
      description: string | null;
      model: string;
      instructions: string;
      availableTools: string | null;
      organization: string;
      project: string;
      createdBy: string;
      updatedBy: string;
      createdAt: string;
      updatedAt: string;
    };

    const sql = `
      SELECT slug, version, name, description, model, instructions, availableTools, organization, project, createdBy, updatedBy, createdAt, updatedAt
      FROM agents
      ${clauses}
      ORDER BY ${order} ASC
      ${limitClause}
    `;
    const res = await this.db
      .prepare(sql)
      .bind(...bind)
      .all<Row>();
    const rows: Row[] = (res.results ?? []) as Row[];

    const items: Agent[] = rows.map((r) => {
      const normalized = {
        ...r,
        description: r.description ?? undefined,
        availableTools: r.availableTools ? (JSON.parse(r.availableTools) as string[]) : undefined,
      };
      return AgentSchema.parse(normalized);
    });

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

  async getLatestBySlug(slug: string): Promise<Agent | null> {
    const item = await this.shared.getLatestBySlug(slug);
    return item ? AgentSchema.parse(item) : null;
  }
}
