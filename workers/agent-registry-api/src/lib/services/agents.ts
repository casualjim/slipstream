import { parse, rcompare } from "semver";
import type { Agent } from "../../schemas/agent";

export class AgentService {
  constructor(private db: D1Database) {}

  async checkSlugVersionExists(slug: string, version: string, organization: string, project: string): Promise<boolean> {
    const result = await this.db
      .prepare("SELECT 1 FROM agents WHERE slug = ? AND version = ? AND organization = ? AND project = ? LIMIT 1")
      .bind(slug, version, organization, project)
      .first();
    return !!result;
  }

  async findByUserOrgs(
    orgIds: string[],
    filters?: {
      organization?: string;
      project?: string;
    },
  ) {
    let query = "SELECT * FROM agents WHERE organization IN (";
    const placeholders = orgIds.map(() => "?").join(",");
    query += placeholders + ")";

    const params = [...orgIds];

    if (filters?.organization) {
      query += " AND organization = ?";
      params.push(filters.organization);
    }

    if (filters?.project) {
      query += " AND project = ?";
      params.push(filters.project);
    }

    return await this.db
      .prepare(query)
      .bind(...params)
      .all();
  }

  async belongsToOrg(agentSlug: string, agentVersion: string, organization: string): Promise<boolean> {
    const result = await this.db
      .prepare("SELECT 1 FROM agents WHERE slug = ? AND version = ? AND organization = ? LIMIT 1")
      .bind(agentSlug, agentVersion, organization)
      .first();
    return !!result;
  }

  async getBySlugVersion(slug: string, version: string): Promise<Agent | null> {
    const result = await this.db
      .prepare("SELECT * FROM agents WHERE slug = ? AND version = ?")
      .bind(slug, version)
      .first();

    if (!result) return null;

    // Parse JSON fields
    if (result.availableTools && typeof result.availableTools === "string") {
      result.availableTools = JSON.parse(result.availableTools as string);
    }

    return result as Agent;
  }

  async create(agent: Agent) {
    // Stringify array fields
    const data = {
      ...agent,
      availableTools: agent.availableTools ? JSON.stringify(agent.availableTools) : null,
    };

    const res = await this.db
      .prepare(
        `INSERT INTO agents (slug, version, name, description, model, instructions, availableTools, organization, project, createdBy, updatedBy, createdAt, updatedAt)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
         RETURNING *`,
      )
      .bind(
        data.slug,
        data.version,
        data.name,
        data.description,
        data.model,
        data.instructions,
        data.availableTools,
        data.organization,
        data.project,
        data.createdBy,
        data.updatedBy,
        data.createdAt,
        data.updatedAt,
      )
      .first<Agent>();

    if (!res) {
      throw new Error("Failed to create agent");
    }

    // Parse availableTools back to array format to match the expected schema
    const result = {
      ...res,
      availableTools: res.availableTools && typeof res.availableTools === 'string'
        ? JSON.parse(res.availableTools)
        : res.availableTools || undefined,
    };

    return result as Agent;
  }

  async update(slug: string, version: string, updateData: Partial<Agent>) {
    // Stringify array fields if present
    const data: any = { ...updateData };
    if (data.availableTools !== undefined) {
      data.availableTools = data.availableTools ? JSON.stringify(data.availableTools) : null;
    }

    // Build update query dynamically
    const fields = Object.keys(data);
    const setClause = fields.map((field) => `${field} = ?`).join(", ");
    const values = fields.map((field) => data[field]);
    values.push(slug, version);

    await this.db
      .prepare(`UPDATE agents SET ${setClause} WHERE slug = ? AND version = ?`)
      .bind(...values)
      .run();

    const updated = await this.getBySlugVersion(slug, version);
    return { success: true, result: updated };
  }

  async delete(slug: string, version: string) {
    await this.db.prepare("DELETE FROM agents WHERE slug = ? AND version = ?").bind(slug, version).run();

    return { success: true };
  }

  async getLatestBySlug(slug: string): Promise<Agent | null> {
    const result = await this.db
      .prepare(
        "SELECT slug, version, name, description, model, instructions, availableTools, organization, project, createdBy, updatedBy, createdAt, updatedAt FROM agents WHERE slug = ?",
      )
      .bind(slug)
      .all<Agent>();

    const agents = (result.results || []) as Agent[];

    if (agents.length === 0) {
      return null;
    }

    // Separate stable and pre-release versions using semver parse
    const stable: Agent[] = [];
    const prerelease: Agent[] = [];

    for (const a of agents) {
      const v = parse(a.version, { includePrerelease: true });
      if (!v) {
        // Skip non-semver entries defensively
        continue;
      }
      if (v.prerelease.length === 0) {
        stable.push(a);
      } else {
        prerelease.push(a);
      }
    }

    const pickLatest = (arr: Agent[]): Agent | null => {
      if (arr.length === 0) return null;
      // Sort descending using semver rcompare (handles build metadata precedence rules)
      const sorted = [...arr].sort((a, b) => rcompare(a.version, b.version));
      return sorted[0] ?? null;
    };

    // Prefer latest stable, otherwise latest prerelease
    const latestStable = pickLatest(stable);
    const chosen = latestStable ?? pickLatest(prerelease);

    if (!chosen) return null;

    // Parse JSON fields
    if (chosen.availableTools && typeof chosen.availableTools === "string") {
      try {
        chosen.availableTools = JSON.parse(chosen.availableTools as string);
      } catch {
        // fall back silently
      }
    }

    return chosen;
  }
}
