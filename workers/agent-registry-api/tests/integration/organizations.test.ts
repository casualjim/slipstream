import { SELF } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";

describe("Organization API Integration Tests", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
  });

  // Tests for POST /api/v1/organizations
  describe("POST /api/v1/organizations", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/organizations`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "Test Organization",
          slug: "test-org",
          description: "A test organization",
        }),
      });
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should create a new organization with valid data", async () => {
      const orgData = {
        name: "Test Organization",
        slug: "test-org",
        description: "A test organization for integration tests",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/organizations`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(orgData),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(201);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        name: orgData.name,
        slug: orgData.slug,
        description: orgData.description,
      });
      expect(body.result.createdAt).toBeDefined();
      expect(body.result.updatedAt).toBeDefined();
    });

    it("should reject duplicate slug", async () => {
      const orgData = {
        name: "Duplicate Test",
        slug: "duplicate-test",
        description: "Testing duplicate slug",
      };

      // Create first organization
      await SELF.fetch(`http://local.test/api/v1/organizations`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(orgData),
      });

      // Try to create another with same slug
      const response = await SELF.fetch(`http://local.test/api/v1/organizations`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(orgData),
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Slug already exists");
    });

    it("should reject invalid slug format", async () => {
      const orgData = {
        name: "Invalid Slug Test",
        slug: "ab", // Too short, must be at least 3 chars
        description: "Testing invalid slug",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/organizations`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(orgData),
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
    });

    it("should reject slug with invalid characters", async () => {
      const orgData = {
        name: "Invalid Slug Test",
        slug: "invalid slug!", // Contains space and special character
        description: "Testing invalid slug characters",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/organizations`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(orgData),
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
    });
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

  // Tests for GET /api/v1/organizations/{slug}
  describe("GET /api/v1/organizations/{slug}", () => {
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

  // Tests for PUT /api/v1/organizations/{slug}
  describe("PUT /api/v1/organizations/{slug}", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/organizations/test-org`, {
        method: "PUT",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "Updated Organization",
          description: "Updated description",
        }),
      });
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should update an organization", async () => {
      // First create an organization
      const orgData = {
        name: "Update Test Organization",
        slug: "update-test-org",
        description: "Original description",
      };

      await SELF.fetch(`http://local.test/api/v1/organizations`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(orgData),
      });

      // Update the organization
      const updateData = {
        name: "Updated Organization Name",
        description: "Updated description for testing",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/organizations/${orgData.slug}`, {
        method: "PUT",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(updateData),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        slug: orgData.slug,
        name: updateData.name,
        description: updateData.description,
      });
      expect(body.result.updatedAt).toBeDefined();
    });

    it("should return 404 if organization to update is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/organizations/non-existent-org`, {
        method: "PUT",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ name: "Updated Name", description: "New description" }),
      });
      const body = await response.json();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
    });

    it("should allow partial updates", async () => {
      // First create an organization
      const orgData = {
        name: "Partial Update Test",
        slug: "partial-update-test",
        description: "Original description",
      };

      await SELF.fetch(`http://local.test/api/v1/organizations`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(orgData),
      });

      // Update only the name
      const response = await SELF.fetch(`http://local.test/api/v1/organizations/${orgData.slug}`, {
        method: "PUT",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ name: "Updated Name Only" }),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result.name).toBe("Updated Name Only");
      expect(body.result.description).toBe("Original description"); // Should remain unchanged
    });
  });

  // Tests for DELETE /api/v1/organizations/{slug}
  describe("DELETE /api/v1/organizations/{slug}", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/organizations/test-org`, {
        method: "DELETE",
      });
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should delete an organization", async () => {
      // First create an organization
      const orgData = {
        name: "Delete Test Organization",
        slug: "delete-test-org",
        description: "Organization to be deleted",
      };

      await SELF.fetch(`http://local.test/api/v1/organizations`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(orgData),
      });

      // Delete the organization
      const response = await SELF.fetch(`http://local.test/api/v1/organizations/${orgData.slug}`, {
        method: "DELETE",
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);

      // Verify it's deleted
      const getResponse = await SELF.fetch(`http://local.test/api/v1/organizations/${orgData.slug}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      expect(getResponse.status).toBe(404);
    });

    it("should return 404 if organization to delete is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/organizations/non-existent-org`, {
        method: "DELETE",
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
    });

    it("should prevent deletion of organization with existing projects", async () => {
      // The seeded Wagyu organization has projects, so we can test this
      const wagyuSlug = "wagyu";

      const response = await SELF.fetch(`http://local.test/api/v1/organizations/${wagyuSlug}`, {
        method: "DELETE",
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Cannot delete organization with existing projects");
    });
  });
});
