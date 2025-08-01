import { parse } from "semver";
import type { Tool } from "../../types";
import { compareSemVer } from "../utils";

export class ToolService {
  constructor(private db: D1Database) {}

  async validateIds(toolIds: string[]): Promise<boolean> {
    if (!toolIds.length) return true;

    // For tools, we need to validate that each tool ID exists
    // Tool IDs should be in format: provider/slug/version
    const results = await Promise.all(
      toolIds.map(async (toolId) => {
        const [provider, slug, version] = toolId.split("/");
        if (!provider || !slug || !version) return false;

        const result = await this.db
          .prepare("SELECT 1 FROM tools WHERE provider = ? AND slug = ? AND version = ? LIMIT 1")
          .bind(provider, slug, version)
          .first();
        return !!result;
      }),
    );

    return results.every(Boolean);
  }

  async getLatestBySlug(provider: string, slug: string): Promise<Tool | null> {
    const result = await this.db
      .prepare(
        "SELECT slug, version, provider, name, description, arguments, createdAt, updatedAt FROM tools WHERE provider = ? AND slug = ?",
      )
      .bind(provider, slug)
      .all();

    if (!result.results || result.results.length === 0) {
      return null;
    }

    // Find the latest version using semantic version comparison
    const tools = result.results as Tool[];
    let latest = tools[0];

    for (let i = 1; i < tools.length; i++) {
      const current = tools[i];
      const currentVersion = parse(current.version);
      const latestVersion = parse(latest.version);

      // Prefer stable versions over pre-release versions
      if (currentVersion && latestVersion) {
        // If current is stable and latest is pre-release, prefer current
        if (!currentVersion.prerelease.length && latestVersion.prerelease.length) {
          latest = current;
          continue;
        }
        // If latest is stable and current is pre-release, skip current
        if (latestVersion.prerelease.length === 0 && currentVersion.prerelease.length) {
          continue;
        }
      }

      // Otherwise, compare normally
      if (compareSemVer(current.version, latest.version) > 0) {
        latest = current;
      }
    }

    return latest;
  }
}
