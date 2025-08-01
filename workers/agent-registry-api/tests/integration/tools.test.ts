import { env, SELF } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";

describe("Tool API Integration Tests", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
  });

  // Tests for POST /api/v1/tools
  describe("POST /api/v1/tools", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "Test Tool",
          slug: "test-tool",
          version: "1.0.0",
          provider: "Local",
        }),
      });
      const body = (await response.json()) as { success: boolean; errors: { message: string }[] };

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should create a new tool with valid data", async () => {
      const toolData = {
        name: "Test Tool",
        slug: "test-tool",
        version: "1.0.0",
        provider: "Local",
        description: "A test tool for integration tests",
        arguments: {
          type: "object",
          properties: {
            input: { type: "string" },
          },
        },
      };

      const response = await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(201);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        name: toolData.name,
        slug: toolData.slug,
        version: toolData.version,
        provider: toolData.provider,
        description: toolData.description,
      });
      expect(body.result.createdAt).toBeDefined();
      expect(body.result.updatedAt).toBeDefined();
    });

    it("should reject invalid provider", async () => {
      const toolData = {
        name: "Test Tool",
        slug: "test-tool",
        version: "1.0.0",
        provider: "INVALID_PROVIDER",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });
      const body = (await response.json()) as { success: boolean };

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
    });

    it("should reject invalid slug format", async () => {
      const toolData = {
        name: "Test Tool",
        slug: "te", // Too short, must be at least 3 chars
        version: "1.0.0",
        provider: "Local",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });
      const body = (await response.json()) as { success: boolean };

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
    });

    describe("semver validation", () => {
      it("should accept valid semver versions", async () => {
        const validVersions = [
          "1.0.0",
          "1.2.3",
          "0.0.1",
          "1.0.0-alpha",
          "1.0.0-alpha.1",
          "1.0.0-0.3.7",
          "1.0.0-x.7.z.92",
          "1.0.0+build.123",
          "1.0.0-alpha+build.456",
          "2.0.0-beta.1+build.789",
          "10.20.30",
          "0.1.0",
          "999.999.999",
        ];

        for (const version of validVersions) {
          const toolData = {
            name: `Test Tool ${version}`,
            slug: `test-tool-${version.replace(/[^a-zA-Z0-9]/g, "-")}`,
            version,
            provider: "Local",
          };

          const response = await SELF.fetch(`http://local.test/api/v1/tools`, {
            method: "POST",
            headers: {
              Authorization: "Bearer test-api-key",
              "Content-Type": "application/json",
            },
            body: JSON.stringify(toolData),
          });

          expect(response.status).toBeLessThan(400),
            `Version ${version} should be valid but was rejected with status ${response.status}`;
        }
      });

      it("should reject invalid semver versions", async () => {
        const invalidVersions = ["", "1.2.3.4.5.6", "latest", "v1.0.0"];

        for (const version of invalidVersions) {
          const toolData = {
            name: `Test Tool ${version || "empty"}`,
            slug: `test-tool-${version.replace(/[^a-zA-Z0-9]/g, "-") || "empty"}`,
            version,
            provider: "Local",
          };

          const response = await SELF.fetch(`http://local.test/api/v1/tools`, {
            method: "POST",
            headers: {
              Authorization: "Bearer test-api-key",
              "Content-Type": "application/json",
            },
            body: JSON.stringify(toolData),
          });

          expect(response.status).toBeGreaterThanOrEqual(400),
            `Version "${version}" should be invalid but was accepted with status ${response.status}`;
        }
      });
    });

    it("should autogenerate slug from name when not provided", async () => {
      const toolData = {
        name: "My Awesome Tool",
        version: "1.0.0",
        provider: "Local",
        description: "A tool with autogenerated slug",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(201);
      expect(body.success).toBe(true);
      expect(body.result.slug).toBe("my-awesome-tool");
      expect(body.result.name).toBe(toolData.name);
    });

    it("should autogenerate slug with special characters", async () => {
      const toolData = {
        name: "Weather API @ Home",
        version: "1.0.0",
        provider: "MCP",
        description: "Weather tool with special chars",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(201);
      expect(body.success).toBe(true);
      expect(body.result.slug).toBe("weather-api-at--home");
    });

    it("should use provided slug when explicitly given", async () => {
      const toolData = {
        name: "My Custom Tool Name",
        slug: "custom-slug-123",
        version: "1.0.0",
        provider: "Local",
        description: "Tool with explicit slug",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(201);
      expect(body.success).toBe(true);
      expect(body.result.slug).toBe("custom-slug-123");
      expect(body.result.name).toBe(toolData.name);
    });
  });

  // Tests for GET /api/v1/tools
  describe("GET /api/v1/tools", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/tools`);
      const body = (await response.json()) as { success: boolean; errors: { message: string }[] };

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should get a list of tools with authorization", async () => {
      // First create a tool
      await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "List Test Tool",
          slug: "list-test-tool",
          version: "1.0.0",
          provider: "Local",
        }),
      });

      const response = await SELF.fetch(`http://local.test/api/v1/tools`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);
      expect(body.result.length).toBeGreaterThanOrEqual(1);
    });

    it("should filter tools by name", async () => {
      // Create a specific tool for testing name filtering
      const specificTool = {
        name: "Unique Filter Test Tool",
        slug: "unique-filter-test",
        version: "1.0.0",
        provider: "Local",
        description: "Tool specifically for name filtering test",
      };

      await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(specificTool),
      });

      // Test filtering by exact name
      const response = await SELF.fetch(
        `http://local.test/api/v1/tools?name=${encodeURIComponent("Unique Filter Test Tool")}`,
        {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        },
      );
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);
      expect(body.result).toHaveLength(1);
      expect(body.result[0].name).toBe("Unique Filter Test Tool");
      expect(body.result[0].slug).toBe("unique-filter-test");
    });

    describe("Pagination", () => {
      let totalToolsCount = 0;

      beforeEach(async () => {
        // Create multiple tools for pagination testing
        const tools = [
          { name: "Tool 1", slug: "tool-1", version: "1.0.0", provider: "Local" },
          { name: "Tool 2", slug: "tool-2", version: "1.0.0", provider: "Local" },
          { name: "Tool 3", slug: "tool-3", version: "1.0.0", provider: "Local" },
          { name: "Tool 4", slug: "tool-4", version: "1.0.0", provider: "Local" },
          { name: "Tool 5", slug: "tool-5", version: "1.0.0", provider: "Local" },
        ];

        for (const tool of tools) {
          await SELF.fetch(`http://local.test/api/v1/tools`, {
            method: "POST",
            headers: {
              Authorization: "Bearer test-api-key",
              "Content-Type": "application/json",
            },
            body: JSON.stringify(tool),
          });
        }

        const countResult = await env.DB.prepare("SELECT COUNT(*) AS count FROM tools").first();
        totalToolsCount = (countResult?.count as number) || 0;
      });

      it("should support basic pagination with page and per_page", async () => {
        const response = await SELF.fetch(`http://local.test/api/v1/tools?page=1&per_page=2`, {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        });
        const body = await response.json<{ success: boolean; result: any[]; result_info?: any }>();

        expect(response.status).toBe(200);
        expect(body.success).toBe(true);
        expect(body.result).toBeInstanceOf(Array);
        expect(body.result).toHaveLength(2);
        expect(body.result_info).toBeDefined();
        expect(body.result_info.count).toBe(2);
        expect(body.result_info.page).toBe(1);
        expect(body.result_info.per_page).toBe(2);
        expect(body.result_info.total_count).toBe(totalToolsCount);
      });

      it("should support pagination with page offset", async () => {
        const response = await SELF.fetch(`http://local.test/api/v1/tools?page=2&per_page=2`, {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        });
        const body = await response.json<{ success: boolean; result: any[]; result_info?: any }>();

        expect(response.status).toBe(200);
        expect(body.success).toBe(true);
        expect(body.result).toBeInstanceOf(Array);
        expect(body.result).toHaveLength(2);
        expect(body.result_info).toBeDefined();
        expect(body.result_info.count).toBe(2);
        expect(body.result_info.page).toBe(2);
        expect(body.result_info.per_page).toBe(2);
        expect(body.result_info.total_count).toBe(totalToolsCount);
      });

      it("should return correct pagination metadata", async () => {
        const response = await SELF.fetch(`http://local.test/api/v1/tools?page=1&per_page=3`, {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        });
        const body = await response.json<{ success: boolean; result: any[]; result_info?: any }>();

        expect(response.status).toBe(200);
        expect(body.success).toBe(true);
        expect(body.result_info).toBeDefined();
        expect(body.result_info.count).toBe(3);
        expect(body.result_info.page).toBe(1);
        expect(body.result_info.per_page).toBe(3);
        expect(body.result_info.total_count).toBe(totalToolsCount);
      });

      it("should handle page greater than total pages", async () => {
        const response = await SELF.fetch(`http://local.test/api/v1/tools?page=100&per_page=10`, {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        });
        const body = await response.json<{ success: boolean; result: any[]; result_info?: any }>();

        expect(response.status).toBe(200);
        expect(body.success).toBe(true);
        expect(body.result).toBeInstanceOf(Array);
        expect(body.result).toHaveLength(0);
        expect(body.result_info.count).toBe(0);
        expect(body.result_info.page).toBe(100);
        expect(body.result_info.per_page).toBe(10);
        expect(body.result_info.total_count).toBe(totalToolsCount);
      });

      it("should combine pagination with search", async () => {
        const response = await SELF.fetch(`http://local.test/api/v1/tools?search=code&page=1&per_page=2`, {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        });
        const body = await response.json<{ success: boolean; result: any[]; result_info?: any }>();

        expect(response.status).toBe(200);
        expect(body.success).toBe(true);
        expect(body.result).toBeInstanceOf(Array);
        expect(body.result.length).toBeLessThanOrEqual(2);
        expect(body.result_info.total_count).toBeGreaterThanOrEqual(0);
      });

      it("should combine pagination with filtering", async () => {
        const response = await SELF.fetch(`http://local.test/api/v1/tools?provider=Local&page=1&per_page=2`, {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        });
        const body = await response.json<{ success: boolean; result: any[]; result_info?: any }>();

        expect(response.status).toBe(200);
        expect(body.success).toBe(true);
        expect(body.result).toBeInstanceOf(Array);
        expect(body.result.length).toBeLessThanOrEqual(2);
        body.result.forEach((tool: any) => {
          expect(tool.provider).toBe("Local");
        });
      });

      it("should combine pagination with ordering", async () => {
        const response = await SELF.fetch(`http://local.test/api/v1/tools?orderBy=name&page=1&per_page=3`, {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        });
        const body = await response.json<{ success: boolean; result: any[]; result_info?: any }>();

        expect(response.status).toBe(200);
        expect(body.success).toBe(true);
        expect(body.result).toBeInstanceOf(Array);
        expect(body.result.length).toBeLessThanOrEqual(3);

        // Check if results are ordered by name
        const names = body.result.map((tool: any) => tool.name);
        const sortedNames = [...names].sort();
        expect(names).toEqual(sortedNames);
      });
    });
  });

  // Tests for GET /api/v1/tools/{provider}/{slug}/{version}
  describe("GET /api/v1/tools/{provider}/{slug}/{version}", () => {
    it("should get a specific tool", async () => {
      // First create a tool
      const toolData = {
        name: "Get Test Tool",
        slug: "get-test-tool",
        version: "2.0.0",
        provider: "MCP",
      };

      await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });

      const response = await SELF.fetch(
        `http://local.test/api/v1/tools/${toolData.provider}/${toolData.slug}/${toolData.version}`,
        {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        },
      );
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        slug: toolData.slug,
        version: toolData.version,
        provider: toolData.provider,
        name: toolData.name,
      });
    });

    it("should return 404 if tool is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/tools/Local/non-existent-tool/1.0.0`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = (await response.json()) as { success: boolean; errors: { message: string }[] };

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Not Found");
    });
  });

  // Tests for PUT /api/v1/tools/{provider}/{slug}/{version}
  describe("PUT /api/v1/tools/{provider}/{slug}/{version}", () => {
    it("should update a tool", async () => {
      // First create a tool
      const toolData = {
        name: "Update Test Tool",
        slug: "update-test-tool",
        version: "1.0.0",
        provider: "Client",
      };

      await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });

      // Update the tool
      const updateData = {
        description: "Updated description",
        arguments: {
          type: "object",
          properties: {
            newInput: { type: "string" },
          },
        },
      };

      const response = await SELF.fetch(
        `http://local.test/api/v1/tools/${toolData.provider}/${toolData.slug}/${toolData.version}`,
        {
          method: "PUT",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: JSON.stringify(updateData),
        },
      );
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        slug: toolData.slug,
        version: toolData.version,
        provider: toolData.provider,
        description: updateData.description,
      });
      expect(body.result.updatedAt).toBeDefined();
    });

    it("should return 404 if tool to update is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/tools/Local/non-existent-tool/1.0.0`, {
        method: "PUT",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ description: "New description" }),
      });
      const body = (await response.json()) as { success: boolean };

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
    });
  });

  // Tests for DELETE /api/v1/tools/{provider}/{slug}/{version}
  describe("DELETE /api/v1/tools/{provider}/{slug}/{version}", () => {
    it("should delete a tool", async () => {
      // First create a tool
      const toolData = {
        name: "Delete Test Tool",
        slug: "delete-test-tool",
        version: "1.0.0",
        provider: "Restate",
      };

      await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });

      // Delete the tool
      const response = await SELF.fetch(
        `http://local.test/api/v1/tools/${toolData.provider}/${toolData.slug}/${toolData.version}`,
        {
          method: "DELETE",
          headers: {
            Authorization: "Bearer test-api-key",
          },
        },
      );
      const body = (await response.json()) as { success: boolean };

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);

      // Verify it's deleted
      const getResponse = await SELF.fetch(
        `http://local.test/api/v1/tools/${toolData.provider}/${toolData.slug}/${toolData.version}`,
        {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        },
      );
      expect(getResponse.status).toBe(404);
    });

    it("should return 404 if tool to delete is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/tools/Local/non-existent-tool/1.0.0`, {
        method: "DELETE",
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = (await response.json()) as { success: boolean };

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
    });
  });

  // Edge cases and failure modes
  describe("Concurrent operations and edge cases", () => {
    it("should handle concurrent tool creation with same slug gracefully", async () => {
      const toolData = {
        name: "Concurrent Test Tool",
        slug: "concurrent-test-tool",
        version: "1.0.0",
        provider: "Local",
        description: "Testing concurrent creation",
      };

      // Fire two identical requests simultaneously
      const [response1, response2] = await Promise.all([
        SELF.fetch("http://local.test/api/v1/tools", {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: JSON.stringify(toolData),
        }),
        SELF.fetch("http://local.test/api/v1/tools", {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: JSON.stringify(toolData),
        }),
      ]);

      const statuses = [response1.status, response2.status].sort();

      // One should succeed (201), one should fail (400+)
      expect(statuses[0]).toBeLessThan(300); // First should succeed
      expect(statuses[1]).toBeGreaterThanOrEqual(400); // Second should fail

      // Verify only one tool was actually created
      const listResponse = await SELF.fetch("http://local.test/api/v1/tools?name=Concurrent Test Tool", {
        headers: { Authorization: "Bearer test-api-key" },
      });
      const body = await listResponse.json<{ result: any[] }>();
      expect(body.result).toHaveLength(1);
    });

    it("should handle database constraint violations gracefully", async () => {
      // First, create a tool successfully
      const toolData = {
        name: "Constraint Test Tool",
        slug: "constraint-test",
        version: "1.0.0",
        provider: "Local",
      };

      await SELF.fetch("http://local.test/api/v1/tools", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });

      // Try to create the same tool again (should violate unique constraint)
      const response = await SELF.fetch("http://local.test/api/v1/tools", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });

      expect(response.status).toBeGreaterThanOrEqual(400);
      const body = (await response.json()) as { success: boolean };
      expect(body.success).toBe(false);
    });

    it("should handle extremely long tool names gracefully", async () => {
      const longName = "A".repeat(1000); // 1000 character name
      const toolData = {
        name: longName,
        version: "1.0.0",
        provider: "Local",
      };

      const response = await SELF.fetch("http://local.test/api/v1/tools", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });

      // Should either accept it or reject with proper error, but not crash
      expect(response.status).toBeLessThan(500);

      if (response.status >= 400) {
        const body = (await response.json()) as { success: boolean };
        expect(body.success).toBe(false);
      }
    });

    it("should handle unicode characters in names and descriptions", async () => {
      const toolData = {
        name: "测试工具 🚀 العربية",
        description: "Description with émojis 🎉 and spéciål chars",
        version: "1.0.0",
        provider: "Local",
      };

      const response = await SELF.fetch("http://local.test/api/v1/tools", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });

      expect(response.status).toBeLessThan(500);

      if (response.status < 300) {
        const body = await response.json<{ success: boolean; result: any }>();
        expect(body.success).toBe(true);
        // Verify the slug was generated correctly
        expect(body.result.slug).toBeDefined();
        expect(body.result.slug.length).toBeGreaterThanOrEqual(3);
      }
    });

    it("should handle malformed request bodies", async () => {
      const responses = await Promise.all([
        // Empty body
        SELF.fetch("http://local.test/api/v1/tools", {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: "",
        }),
        // Invalid JSON
        SELF.fetch("http://local.test/api/v1/tools", {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: "{invalid json",
        }),
        // Missing required fields
        SELF.fetch("http://local.test/api/v1/tools", {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: JSON.stringify({}),
        }),
      ]);

      // All should fail gracefully, not crash
      responses.forEach((response) => {
        expect(response.status).toBeGreaterThanOrEqual(400);
        expect(response.status).toBeLessThan(500);
      });
    });

    it("should handle malformed JSON in arguments field", async () => {
      const toolData = {
        name: "Malformed Args Tool",
        version: "1.0.0",
        provider: "Local",
        arguments: "not-valid-json-schema",
      };

      const response = await SELF.fetch("http://local.test/api/v1/tools", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(toolData),
      });

      // Should handle gracefully - either accept it as string or reject with validation error
      expect(response.status).toBeLessThan(500);
    });
  });

  // Tests for GET /api/v1/tools/{slug}/latest
  describe("GET /api/v1/tools/{slug}/latest", () => {
    it("should get the latest version of a tool", async () => {
      // Create multiple versions of the same tool
      const toolVersions = [
        { name: "Latest Test Tool", slug: "latest-test-tool", version: "1.0.0", provider: "Local" },
        { name: "Latest Test Tool", slug: "latest-test-tool", version: "1.1.0", provider: "Local" },
        { name: "Latest Test Tool", slug: "latest-test-tool", version: "2.0.0", provider: "MCP" },
      ];

      for (const toolData of toolVersions) {
        await SELF.fetch(`http://local.test/api/v1/tools`, {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: JSON.stringify(toolData),
        });
      }

      const response = await SELF.fetch(`http://local.test/api/v1/tools/MCP/latest-test-tool`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        slug: "latest-test-tool",
        version: "2.0.0", // Should return the highest version
        name: "Latest Test Tool",
        provider: "MCP",
      });
    });

    it("should return 404 if tool slug is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/tools/Local/non-existent-tool`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; errors: { message: string }[] }>();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Not Found");
    });

    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/tools/MCP/latest-test-tool`);
      const body = await response.json<{ success: boolean; errors: { message: string }[] }>();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should handle pre-release versions correctly", async () => {
      // Create versions including pre-releases
      const toolVersions = [
        { name: "Prerelease Test Tool", slug: "prerelease-test-tool", version: "1.0.0", provider: "Local" },
        { name: "Prerelease Test Tool", slug: "prerelease-test-tool", version: "2.0.0-alpha", provider: "Local" },
        { name: "Prerelease Test Tool", slug: "prerelease-test-tool", version: "2.0.0-beta", provider: "Local" },
      ];

      for (const toolData of toolVersions) {
        await SELF.fetch(`http://local.test/api/v1/tools`, {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: JSON.stringify(toolData),
        });
      }

      const response = await SELF.fetch(`http://local.test/api/v1/tools/Local/prerelease-test-tool`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      // Should return the stable version, not pre-release
      expect(body.result.version).toBe("1.0.0");
    });

    it("should handle build metadata correctly", async () => {
      // Create versions with build metadata
      const toolVersions = [
        { name: "Build Metadata Test Tool", slug: "build-metadata-test-tool", version: "1.0.0", provider: "Local" },
        {
          name: "Build Metadata Test Tool",
          slug: "build-metadata-test-tool",
          version: "1.0.0+build.1",
          provider: "Local",
        },
        {
          name: "Build Metadata Test Tool",
          slug: "build-metadata-test-tool",
          version: "1.0.0+build.2",
          provider: "Local",
        },
      ];

      for (const toolData of toolVersions) {
        await SELF.fetch(`http://local.test/api/v1/tools`, {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: JSON.stringify(toolData),
        });
      }

      const response = await SELF.fetch(`http://local.test/api/v1/tools/Local/build-metadata-test-tool`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      // Should return one of the versions with build metadata (they're considered equal in precedence)
      expect(["1.0.0", "1.0.0+build.1", "1.0.0+build.2"]).toContain(body.result.version);
    });

    it("should handle multiple providers with same slug", async () => {
      // Create tools with same slug but different providers
      const toolVersions = [
        { name: "Multi Provider Tool", slug: "multi-provider-tool", version: "1.0.0", provider: "Local" },
        { name: "Multi Provider Tool", slug: "multi-provider-tool", version: "2.0.0", provider: "MCP" },
        { name: "Multi Provider Tool", slug: "multi-provider-tool", version: "1.5.0", provider: "Client" },
      ];

      for (const toolData of toolVersions) {
        await SELF.fetch(`http://local.test/api/v1/tools`, {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: JSON.stringify(toolData),
        });
      }

      const response = await SELF.fetch(`http://local.test/api/v1/tools/MCP/multi-provider-tool`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      // Should return the highest version across all providers
      expect(body.result.version).toBe("2.0.0");
      expect(body.result.provider).toBe("MCP");
    });
  });
});
