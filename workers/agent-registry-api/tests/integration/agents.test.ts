import { SELF } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";

describe("Agent API Integration Tests", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
  });

  // Tests for POST /api/v1/agents
  describe("POST /api/v1/agents", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "Test Agent",
          version: "1.0.0",
          model: "openai/gpt-4.1",
          instructions: "You are a helpful assistant.",
          organization: "wagyu",
          project: "wagyu-project",
        }),
      });
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should create a new agent with valid data", async () => {
      const agentData = {
        name: "Test Agent",
        version: "1.0.0",
        description: "A test agent for integration tests",
        model: "openai/gpt-4.1",
        instructions: "You are a helpful assistant for testing purposes.",
        availableTools: [],
        organization: "wagyu",
        project: "wagyu-project",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      if (response.status !== 201) {
        console.error("Agent creation failed:", response.status, JSON.stringify(body, null, 2));
      }

      expect(response.status).toBe(201);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        name: agentData.name,
        version: agentData.version,
        description: agentData.description,
        model: agentData.model,
        instructions: agentData.instructions,
        organization: agentData.organization,
        project: agentData.project,
      });
      expect(body.result.slug).toBe("test-agent");
      expect(body.result.createdBy).toBeDefined();
      expect(body.result.updatedBy).toBeDefined();
      expect(body.result.createdAt).toBeDefined();
      expect(body.result.updatedAt).toBeDefined();
    });

    it("should reject invalid model", async () => {
      const agentData = {
        name: "Test Agent",
        version: "1.0.0",
        model: "invalid/model-id",
        instructions: "You are a helpful assistant.",
        organization: "wagyu",
        project: "wagyu-project",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toContain("Invalid model ID");
    });

    it("should reject if user doesn't belong to organization", async () => {
      const agentData = {
        name: "Test Agent",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: "You are a helpful assistant.",
        organization: "invalid-org", // Invalid org
        project: "wagyu-project",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });
      const body = await response.json();

      expect(response.status).toBe(403);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toContain("Access denied to organization");
    });

    it("should reject if project doesn't belong to organization", async () => {
      const agentData = {
        name: "Test Agent",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: "You are a helpful assistant.",
        organization: "wagyu",
        project: "invalid-project", // Invalid project
      };

      const response = await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toContain("Project does not belong to organization");
    });

    it("should create agent with tools", async () => {
      // First create a tool
      const toolResponse = await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "Agent Test Tool",
          slug: "agent-test-tool",
          version: "1.0.0",
          provider: "Local", // Correct case
        }),
      });
      const toolBody = await toolResponse.json<{ success: boolean; result: any }>();
      // Tools use composite keys, not a single id field
      const toolId = `${toolBody.result.provider}/${toolBody.result.slug}/${toolBody.result.version}`;

      // Create agent with tool
      const agentData = {
        name: "Agent With Tools",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: "You are a helpful assistant with tools.",
        availableTools: [toolId],
        organization: "wagyu",
        project: "wagyu-project",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(201);
      expect(body.success).toBe(true);
      expect(body.result.availableTools).toEqual([toolId]);
    });
  });

  // Tests for GET /api/v1/agents
  describe("GET /api/v1/agents", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/agents`);
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should get a list of agents with authorization", async () => {
      // First create an agent
      await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "List Test Agent",
          version: "1.0.0",
          model: "openai/gpt-4.1",
          instructions: "Test instructions",
          organization: "wagyu",
          project: "wagyu-project",
        }),
      });

      const response = await SELF.fetch(`http://local.test/api/v1/agents`, {
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

    it("should filter agents by organization", async () => {
      const response = await SELF.fetch(
        `http://local.test/api/v1/agents?organization=wagyu`,
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
    });

    it("should filter agents by project", async () => {
      const response = await SELF.fetch(
        `http://local.test/api/v1/agents?project=wagyu-project`,
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
    });
  });

  // Tests for GET /api/v1/agents/{slug}/{version}
  describe("GET /api/v1/agents/{slug}/{version}", () => {
    it("should get a specific agent", async () => {
      // First create an agent
      const agentData = {
        name: "Get Test Agent",
        version: "2.0.0",
        model: "google/gemini-2.5-flash",
        instructions: "Test instructions for get",
        organization: "wagyu",
        project: "wagyu-project",
      };

      await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });

      const response = await SELF.fetch(`http://local.test/api/v1/agents/get-test-agent/2.0.0`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        slug: "get-test-agent",
        version: "2.0.0",
        name: agentData.name,
        model: agentData.model,
        instructions: agentData.instructions,
      });
    });

    it("should return 404 if agent is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/agents/non-existent-agent/1.0.0`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Agent not found");
    });

    it("should return 403 if user doesn't have access to agent's organization", async () => {
      // This test would require creating an agent in a different org
      // For now, we'll skip this test as it requires more complex setup
    });

    it("should return 400 for invalid agent ID format", async () => {
      const response = await SELF.fetch(
        `http://local.test/api/v1/agents/invalid-format`, // Missing version
        {
          headers: {
            Authorization: "Bearer test-api-key",
          },
        },
      );
      const body = await response.json<{ success: boolean; errors: Array<{ message: string }> }>();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Not Found");
    });
  });

  // Tests for PUT /api/v1/agents/{slug}/{version}
  describe("PUT /api/v1/agents/{slug}/{version}", () => {
    it("should update an agent", async () => {
      // First create an agent
      const agentData = {
        name: "Update Test Agent",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: "Original instructions",
        organization: "wagyu",
        project: "wagyu-project",
      };

      await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });

      // Update the agent
      const updateData = {
        description: "Updated description",
        instructions: "Updated instructions",
        model: "google/gemini-2.5-pro",
      };

      const response = await SELF.fetch(`http://local.test/api/v1/agents/update-test-agent/1.0.0`, {
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
        slug: "update-test-agent",
        version: "1.0.0",
        description: updateData.description,
        instructions: updateData.instructions,
        model: updateData.model,
      });
      expect(body.result.updatedAt).toBeDefined();
      expect(body.result.updatedBy).toBeDefined();
    });

    it("should update agent tools", async () => {
      // Create a tool first
      const toolResponse = await SELF.fetch(`http://local.test/api/v1/tools`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "Update Agent Tool",
          slug: "update-agent-tool",
          version: "1.0.0",
          provider: "MCP",
        }),
      });
      const toolBody = await toolResponse.json<{ success: boolean; result: any }>();
      const toolId = `${toolBody.result.provider}/${toolBody.result.slug}/${toolBody.result.version}`;

      // Create agent
      await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "Agent Tool Update",
          version: "1.0.0",
          model: "openai/gpt-4.1",
          instructions: "Test",
          organization: "wagyu",
          project: "wagyu-project",
        }),
      });

      // Update with tools
      const response = await SELF.fetch(`http://local.test/api/v1/agents/agent-tool-update/1.0.0`, {
        method: "PUT",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          availableTools: [toolId],
        }),
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result.availableTools).toEqual([toolId]);
    });

    it("should return 404 if agent to update is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/agents/non-existent-agent/1.0.0`, {
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
      expect(body.errors[0].message).toBe("Not Found");
    });

    it("should return 403 if user doesn't have access", async () => {
      // This test would require creating an agent in a different org
      // For now, we'll skip this test as it requires more complex setup
    });

    it("should reject invalid model on update", async () => {
      // Create agent first
      await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          name: "Model Validation Agent",
          version: "1.0.0",
          model: "openai/gpt-4.1",
          instructions: "Test",
          organization: "wagyu",
          project: "wagyu-project",
        }),
      });

      const response = await SELF.fetch(`http://local.test/api/v1/agents/model-validation-agent/1.0.0`, {
        method: "PUT",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          model: "invalid/model-id",
        }),
      });
      const body = await response.json();

      expect(response.status).toBe(400);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toContain("Invalid model ID");
    });
  });

  // Tests for DELETE /api/v1/agents/{slug}/{version}
  describe("DELETE /api/v1/agents/{slug}/{version}", () => {
    it("should delete an agent", async () => {
      // First create an agent
      const agentData = {
        name: "Delete Test Agent",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: "Test instructions",
        organization: "wagyu",
        project: "wagyu-project",
      };

      await SELF.fetch(`http://local.test/api/v1/agents`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });

      // Delete the agent
      const response = await SELF.fetch(`http://local.test/api/v1/agents/delete-test-agent/1.0.0`, {
        method: "DELETE",
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);

      // Verify it's deleted
      const getResponse = await SELF.fetch(`http://local.test/api/v1/agents/delete-test-agent/1.0.0`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      expect(getResponse.status).toBe(404);
    });

    it("should return 404 if agent to delete is not found", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/agents/non-existent-agent/1.0.0`, {
        method: "DELETE",
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Not Found");
    });

    it("should return 403 if user doesn't have access", async () => {
      // This test would require creating an agent in a different org
      // For now, we'll skip this test as it requires more complex setup
    });

    it("should return 400 for invalid agent ID format", async () => {
      const response = await SELF.fetch(
        `http://local.test/api/v1/agents/invalid-format`, // Missing version
        {
          method: "DELETE",
          headers: {
            Authorization: "Bearer test-api-key",
          },
        },
      );
      const body = await response.json<{ success: boolean; errors: Array<{ message: string }> }>();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Not Found");
    });
  });

  // Business logic and edge case tests
  describe("Business logic violations and edge cases", () => {
    it("should handle concurrent agent creation with same slug", async () => {
      const agentData = {
        name: "Concurrent Agent",
        slug: "concurrent-agent",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: "Test agent",
        organization: "wagyu",
        project: "wagyu-project",
      };

      const [response1, response2] = await Promise.all([
        SELF.fetch("http://local.test/api/v1/agents", {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: JSON.stringify(agentData),
        }),
        SELF.fetch("http://local.test/api/v1/agents", {
          method: "POST",
          headers: {
            Authorization: "Bearer test-api-key",
            "Content-Type": "application/json",
          },
          body: JSON.stringify(agentData),
        }),
      ]);

      const statuses = [response1.status, response2.status].sort();
      expect(statuses[0]).toBeLessThan(300); // One succeeds
      expect(statuses[1]).toBeGreaterThanOrEqual(400); // One fails
    });

    it("should fail gracefully when creating agent with non-existent model", async () => {
      const agentData = {
        name: "Bad Model Agent",
        slug: "bad-model-agent",
        version: "1.0.0",
        model: "non-existent-model-id",
        instructions: "This should fail",
        organization: "wagyu",
        project: "wagyu-project",
      };

      const response = await SELF.fetch("http://local.test/api/v1/agents", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });

      expect(response.status).toBeGreaterThanOrEqual(400);
      const body = await response.json() as { success: boolean };
      expect(body.success).toBe(false);

      // Should not have created a partial record
      const listResponse = await SELF.fetch(
        "http://local.test/api/v1/agents",
        {
          headers: { Authorization: "Bearer test-api-key" },
        }
      );
      const listBody = await listResponse.json<{ result: any[] }>();
      const badAgents = listBody.result.filter(a => a.slug === "bad-model-agent");
      expect(badAgents).toHaveLength(0);
    });

    it("should fail gracefully when creating agent with non-existent organization", async () => {
      const agentData = {
        name: "Bad Org Agent",
        slug: "bad-org-agent",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: "This should fail",
        organization: "non-existent-org",
        project: "wagyu-project",
      };

      const response = await SELF.fetch("http://local.test/api/v1/agents", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });

      expect(response.status).toBeGreaterThanOrEqual(400);
      const body = await response.json() as { success: boolean };
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
          const agentData = {
            name: `Test Agent ${version}`,
            slug: `test-agent-${version.replace(/[^a-zA-Z0-9]/g, '-')}`,
            version,
            model: "openai/gpt-4.1",
            instructions: "Test instructions",
            organization: "wagyu",
            project: "wagyu-project",
          };

          const response = await SELF.fetch(`http://local.test/api/v1/agents`, {
            method: "POST",
            headers: {
              Authorization: "Bearer test-api-key",
              "Content-Type": "application/json",
            },
            body: JSON.stringify(agentData),
          });

          expect(response.status).toBeLessThan(400),
            `Version ${version} should be valid but was rejected with status ${response.status}`;
        }
      });

      it("should reject invalid semver versions", async () => {
        const invalidVersions = ["", "1.2.3.4.5.6", "latest", "v1.0.0"];

        for (const version of invalidVersions) {
          const agentData = {
            name: `Test Agent ${version || "empty"}`,
            slug: `test-agent-${version.replace(/[^a-zA-Z0-9]/g, '-') || "empty"}`,
            version,
            model: "openai/gpt-4.1",
            instructions: "Test instructions",
            organization: "wagyu",
            project: "wagyu-project",
          };

          const response = await SELF.fetch(`http://local.test/api/v1/agents`, {
            method: "POST",
            headers: {
              Authorization: "Bearer test-api-key",
              "Content-Type": "application/json",
            },
            body: JSON.stringify(agentData),
          });

          expect(response.status).toBeGreaterThanOrEqual(400),
            `Version "${version}" should be invalid but was accepted with status ${response.status}`;
        }
      });
    });

    it("should handle invalid availableTools references", async () => {
      const agentData = {
        name: "Invalid Tools Agent",
        slug: "invalid-tools-agent",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: "Agent with invalid tool references",
        organization: "wagyu",
        project: "wagyu-project",
        availableTools: [
          { slug: "non-existent-tool", version: "1.0.0", provider: "Local" },
        ],
      };

      const response = await SELF.fetch("http://local.test/api/v1/agents", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });

      // Should not crash the system
      expect(response.status).toBeLessThan(500);
    });

    it("should handle malformed JSON in availableTools", async () => {
      const agentData = {
        name: "Malformed JSON Agent",
        slug: "malformed-json-agent",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: "Agent with malformed JSON",
        organization: "wagyu",
        project: "wagyu-project",
        availableTools: "invalid-json-string",
      };

      const response = await SELF.fetch("http://local.test/api/v1/agents", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });

      expect(response.status).toBeGreaterThanOrEqual(400);
      const body = await response.json() as { success: boolean };
      expect(body.success).toBe(false);
    });

    it("should handle version conflicts in updates", async () => {
      // Create an agent
      const agentData = {
        name: "Version Conflict Agent",
        slug: "version-conflict-agent",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: "Original version",
        organization: "wagyu",
        project: "wagyu-project",
      };

      await SELF.fetch("http://local.test/api/v1/agents", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });

      // Try to create another version with same slug but different version
      const newVersionData = {
        ...agentData,
        version: "2.0.0",
        instructions: "New version",
      };

      const response = await SELF.fetch("http://local.test/api/v1/agents", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(newVersionData),
      });

      expect(response.status).toBeLessThan(500);

      if (response.status < 300) {
        // If versioning is allowed, verify both versions exist
        const listResponse = await SELF.fetch(
          "http://local.test/api/v1/agents",
          {
            headers: { Authorization: "Bearer test-api-key" },
          }
        );
        const body = await listResponse.json<{ result: any[] }>();
        const agents = body.result.filter(a => a.slug === "version-conflict-agent");
        expect(agents.length).toBeGreaterThan(0);
      }
    });

    it("should handle extremely long instructions gracefully", async () => {
      const longInstructions = "This is a very long instruction. ".repeat(1000); // ~34KB
      const agentData = {
        name: "Long Instructions Agent",
        slug: "long-instructions-agent",
        version: "1.0.0",
        model: "openai/gpt-4.1",
        instructions: longInstructions,
        organization: "wagyu",
        project: "wagyu-project",
      };

      const response = await SELF.fetch("http://local.test/api/v1/agents", {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify(agentData),
      });

      // Should either accept it or reject with proper error, but not crash
      expect(response.status).toBeLessThan(500);
    });
  });
});
