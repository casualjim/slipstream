export class ToolService {
  constructor(private db: D1Database) {}

  async validateIds(toolIds: string[]): Promise<boolean> {
    if (!toolIds.length) return true;

    // For tools, we need to validate that each tool ID exists
    // Tool IDs should be in format: provider/slug/version
    const results = await Promise.all(
      toolIds.map(async (toolId) => {
        const [provider, slug, version] = toolId.split('/');
        if (!provider || !slug || !version) return false;
        
        const result = await this.db
          .prepare("SELECT 1 FROM tools WHERE provider = ? AND slug = ? AND version = ? LIMIT 1")
          .bind(provider, slug, version)
          .first();
        return !!result;
      })
    );

    return results.every(Boolean);
  }
}
