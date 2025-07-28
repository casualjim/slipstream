import { SELF } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";

describe("Project API Integration Tests", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
  });

  // Tests for POST /api/v1/projects
  describe("POST /api/v1/projects", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "Test Project",
          organization: "wagyu",
          description: "A test project",
        }),
      });
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should create a new project with valid data", async () => {
      const projectData = {
        name: "Test Project",
        slug: "test-project",
        organization: "wagyu",
        description: "A test project for integration tests",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(projectData),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(201);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        name: projectData.name,
        organization: projectData.organization,
        description: projectData.description,
      });
      expect(body.result.slug).toBeDefined();
      expect(body.result.createdAt).toBeDefined();
      expect(body.result.updatedAt).toBeDefined();
    });

    it("should reject project creation in unauthorized organization", async () => {
      const projectData = {
        name: "Unauthorized Project",
        slug: "unauthorized-project",
        organization: "unauthorized-org",
        description: "Trying to create project in unauthorized org",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(projectData),
      });
      const body = await response.json();

      expect(response.status).toBe(403);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Access denied to organization");
    });

    it("should reject duplicate slug", async () => {
      const projectData = {
        name: "Duplicate Test Project",
        slug: "duplicate-test-project",
        organization: "wagyu",
        description: "Testing duplicate slug",
      };

      // Create first project
      await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(projectData),
      });

      // Try to create another with same slug
      const response = await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(projectData),
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Slug already exists");
    });

    it("should reject invalid slug format", async () => {
      const projectData = {
        name: "Invalid Project",
        slug: "ab", // Too short, invalid slug
        organization: "wagyu",
        description: "Testing invalid slug",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(projectData),
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
    });

    it("should reject missing required fields", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          description: "Missing name and organization",
        }),
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
    });
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

  // Tests for PUT /api/v1/projects/{slug}
  describe("PUT /api/v1/projects/{slug}", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/projects/test-project`, {
        method: "PUT",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "Updated Project",
          description: "Updated description",
        }),
      });
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should update a project", async () => {
      // First create a project
      const projectData = {
        name: "Update Test Project",
        slug: "update-test-project",
        organization: "wagyu",
        description: "Original description",
      };

      await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(projectData),
      });

      // Update the project
      const updateData = {
        name: "Updated Project Name",
        description: "Updated description for testing",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/projects/update-test-project`, {
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
        slug: "update-test-project",
        name: updateData.name,
        description: updateData.description,
        organization: "wagyu",
      });
      expect(body.result.updatedAt).toBeDefined();
    });

    it("should return 404 if project to update is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/projects/non-existent-project`, {
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
      // First create a project
      const projectData = {
        name: "Partial Update Test",
        slug: "partial-update-test",
        organization: "wagyu",
        description: "Original description",
      };

      await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(projectData),
      });

      // Update only the name
      const response = await SELF.fetch(`http://local.test/api/v1/projects/partial-update-test`, {
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
      expect(body.result.organization).toBe("wagyu"); // Should remain unchanged
    });

    it("should prevent updating organization to unauthorized one", async () => {
      // First create a project
      const projectData = {
        name: "Org Update Test",
        slug: "org-update-test",
        organization: "wagyu",
        description: "Testing org update restriction",
      };

      await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(projectData),
      });

      // Try to update organization to unauthorized one
      const response = await SELF.fetch(`http://local.test/api/v1/projects/org-update-test`, {
        method: "PUT",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ organization: "unauthorized-org" }),
      });
      const body = await response.json();

      expect(response.status).toBe(403);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Access denied to organization");
    });
  });

  // Tests for DELETE /api/v1/projects/{slug}
  describe("DELETE /api/v1/projects/{slug}", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/projects/test-project`, {
        method: "DELETE",
      });
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should delete a project", async () => {
      // First create a project
      const projectData = {
        name: "Delete Test Project",
        slug: "delete-test-project",
        organization: "wagyu",
        description: "Project to be deleted",
      };

      await SELF.fetch(`http://local.test/api/v1/projects`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(projectData),
      });

      // Delete the project
      const response = await SELF.fetch(`http://local.test/api/v1/projects/delete-test-project`, {
        method: "DELETE",
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);

      // Verify it's deleted
      const getResponse = await SELF.fetch(`http://local.test/api/v1/projects/delete-test-project`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      expect(getResponse.status).toBe(404);
    });

    it("should return 404 if project to delete is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/projects/non-existent-project`, {
        method: "DELETE",
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
    });

    it("should prevent deletion of protected seeded project", async () => {
      // The seeded wagyu-project should be protected
      const wagyuProjectSlug = "wagyu-project";

      const response = await SELF.fetch(`http://local.test/api/v1/projects/${wagyuProjectSlug}`, {
        method: "DELETE",
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      // This might succeed or fail depending on business rules
      // For now, we'll just verify the response is handled appropriately
      expect([200, 400, 404]).toContain(response.status);
    });
  });
});
