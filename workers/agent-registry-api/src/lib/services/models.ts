export class ModelService {
  constructor(private db: D1Database) {}

  async exists(modelId: string): Promise<boolean> {
    const result = await this.db.prepare("SELECT 1 FROM model_providers WHERE id = ? LIMIT 1").bind(modelId).first();
    return !!result;
  }
}
