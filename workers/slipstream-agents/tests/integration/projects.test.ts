import { SELF } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";

describe("Project API Integration Tests", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
  });

  // Tests for GET /api/v1/projects
  describe("GET /api/v1/projects", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/projects`);
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should get a list of projects with authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/projects`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);
      // Should include the seeded Wagyu project
      expect(body.result.length).toBeGreaterThanOrEqual(1);
      expect(body.result.some((project) => project.slug === "wagyu-project")).toBe(true);
    });

    it("should filter projects by organization", async () => {
      const wagyuOrgSlug = "wagyu";
      const response = await SELF.fetch(`http://local.test/api/v1/projects?organization=${wagyuOrgSlug}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);
      // All projects should belong to the Wagyu organization
      expect(body.result.every((project) => project.organization === wagyuOrgSlug)).toBe(true);
    });

    it("should filter projects by name", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/projects?name=Default`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);
      // Should return projects with "Default" in the name
      expect(body.result.some((project) => project.name === "Default")).toBe(true);
    });

    it("should filter projects by slug", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/projects?slug=wagyu-project`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);
      // Should return the wagyu-project
      expect(body.result.some((project) => project.slug === "wagyu-project")).toBe(true);
    });
  });

  // Tests for GET /api/v1/projects/{slug}
  describe("GET /api/v1/projects/{slug}", () => {
    it("should get the seeded Wagyu project by slug", async () => {
      const wagyuProjectSlug = "wagyu-project";

      const response = await SELF.fetch(`http://local.test/api/v1/projects/${wagyuProjectSlug}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        slug: "wagyu-project",
        name: "Default",
        description: "Default project under the Wagyu organization",
        organization: "wagyu",
      });
    });

    it("should return 404 if project is not found", async () => {
      const nonExistentSlug = "non-existent-project";
      const response = await SELF.fetch(`http://local.test/api/v1/projects/${nonExistentSlug}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Not Found");
    });

    it("should return 400 for invalid project slug format", async () => {
      const invalidSlug = "not a valid slug!"; // Contains spaces and special characters
      const response = await SELF.fetch(`http://local.test/api/v1/projects/${encodeURIComponent(invalidSlug)}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
      // The exact error message may vary based on D1AutoEndpoint validation
    });
  });
});
