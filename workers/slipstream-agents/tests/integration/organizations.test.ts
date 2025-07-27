import { SELF } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";

describe("Organization API Integration Tests", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
  });

  // Tests for GET /api/v1/organizations
  describe("GET /api/v1/organizations", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/organizations`);
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should get a list of organizations with authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/organizations`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);
      // Should include the seeded Wagyu organization
      expect(body.result.length).toBeGreaterThanOrEqual(1);
      expect(body.result.some((org) => org.slug === "wagyu")).toBe(true);
    });
  });

  // Tests for GET /api/v1/organizations/{id}
  describe("GET /api/v1/organizations/{id}", () => {
    it("should get the seeded Wagyu organization by slug", async () => {
      const wagyuSlug = "wagyu";

      const response = await SELF.fetch(`http://local.test/api/v1/organizations/${wagyuSlug}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        slug: "wagyu",
        name: "Wagyu",
        description: "The family organization",
      });
    });

    it("should return 404 if organization is not found", async () => {
      const nonExistentSlug = "non-existent-org";
      const response = await SELF.fetch(`http://local.test/api/v1/organizations/${nonExistentSlug}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Not Found");
    });
  });
});
