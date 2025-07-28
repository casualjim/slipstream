export class ProjectService {
  constructor(private db: D1Database) {}

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
