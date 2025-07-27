import { SELF } from "cloudflare:test";
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
      const body = await response.json();

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
      const body = await response.json();

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
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
    });
  });

  // Tests for GET /api/v1/tools
  describe("GET /api/v1/tools", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/tools`);
      const body = await response.json();

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
      const response = await SELF.fetch(`http://local.test/api/v1/tools?name=List Test Tool`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);
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
      const body = await response.json();

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
      const body = await response.json();

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
      const body = await response.json();

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
      const body = await response.json();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
    });
  });
});
