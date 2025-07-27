import type { Agent } from "../../types";

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

    await this.db
      .prepare(
        `INSERT INTO agents (slug, version, name, description, model, instructions, availableTools, organization, project, createdBy, updatedBy, createdAt, updatedAt)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)`,
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
      .run();

    return { success: true, result: agent };
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
}
