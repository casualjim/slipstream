export class OrganizationService {
  constructor(private db: D1Database) {}

  async checkSlugExists(slug: string): Promise<boolean> {
    const result = await this.db.prepare("SELECT 1 FROM organizations WHERE slug = ? LIMIT 1").bind(slug).first();
    return !!result;
  }

  async hasProjects(orgId: string): Promise<boolean> {
    const result = await this.db.prepare("SELECT 1 FROM projects WHERE organizationId = ? LIMIT 1").bind(orgId).first();
    return !!result;
  }

  async findByIds(ids: string[]) {
    if (!ids.length) return { results: [] };

    const placeholders = ids.map(() => "?").join(",");
    return await this.db
      .prepare(`SELECT * FROM organizations WHERE id IN (${placeholders})`)
      .bind(...ids)
      .all();
  }
}
