import { SELF } from "cloudflare:test";
import { beforeEach, describe, expect, it, vi } from "vitest";

describe("Model API Integration Tests", () => {
  beforeEach(async () => {
    vi.clearAllMocks();
  });

  // Tests for GET /api/v1/models
  describe("GET /api/v1/models", () => {
    it("should return 401 without authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/models`);
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });

    it("should get a list of models with authorization", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/models`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);
      // Should include the seeded models
      expect(body.result.length).toBeGreaterThanOrEqual(1);

      // Verify some expected models exist
      const modelIds = body.result.map((m) => m.id);
      expect(modelIds).toContain("openai/gpt-4.1");
      expect(modelIds).toContain("google/gemini-2.5-flash");
      expect(modelIds).toContain("anthropic/claude-sonnet-4-0");
    });

    it("should return models with expected structure", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/models`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);

      // Check structure of first model
      const firstModel = body.result[0];
      expect(firstModel).toHaveProperty("id");
      expect(firstModel).toHaveProperty("name");
      expect(firstModel).toHaveProperty("provider");
      expect(firstModel).toHaveProperty("contextSize");
      expect(firstModel).toHaveProperty("capabilities");
      expect(firstModel).toHaveProperty("inputModalities");
      expect(firstModel).toHaveProperty("outputModalities");

      // Verify capabilities and modalities are arrays
      expect(Array.isArray(firstModel.capabilities)).toBe(true);
      expect(Array.isArray(firstModel.inputModalities)).toBe(true);
      expect(Array.isArray(firstModel.outputModalities)).toBe(true);
    });

    it("should filter models by provider", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/models?provider=Google`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);

      // All results should be from Google provider
      body.result.forEach((model) => {
        expect(model.provider).toBe("Google");
      });
    });

    it("should filter models by name", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/models?name=GPT`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any[] }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toBeInstanceOf(Array);

      // Results should contain "GPT" in the name
      body.result.forEach((model) => {
        expect(model.name.toLowerCase()).toContain("gpt");
      });
    });
  });

  // Tests for GET /api/v1/models/{id}
  describe("GET /api/v1/models/{id}", () => {
    it("should get a specific model by ID", async () => {
      const modelId = "openai/gpt-4.1";

      const response = await SELF.fetch(`http://local.test/api/v1/models/${modelId}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        id: "openai/gpt-4.1",
        name: "GPT-4.1",
        provider: "OpenAI",
        capabilities: expect.arrayContaining(["chat", "completion", "function_calling", "structured_output"]),
        inputModalities: expect.arrayContaining(["text", "image"]),
        outputModalities: ["text"],
        contextSize: 1048576,
        maxTokens: 32768,
      });
    });

    it("should get Google Gemini model", async () => {
      const modelId = "google/gemini-2.5-flash";

      const response = await SELF.fetch(`http://local.test/api/v1/models/${modelId}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        id: "google/gemini-2.5-flash",
        name: "Gemini 2.5 Flash",
        provider: "Google",
        description: "Google's Gemini 2.5 Flash model, optimized for fast responses.",
        capabilities: expect.arrayContaining(["chat", "completion", "function_calling", "structured_output", "search"]),
        inputModalities: expect.arrayContaining(["text", "image", "video", "audio"]),
        outputModalities: ["text"],
        contextSize: 1048576,
        maxTokens: 65536,
      });
    });

    it("should get DeepSeek model with dialect", async () => {
      const modelId = "deepseek-ai/DeepSeek-R1-0528";

      const response = await SELF.fetch(`http://local.test/api/v1/models/${modelId}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result).toMatchObject({
        id: "deepseek-ai/DeepSeek-R1-0528",
        name: "DeepSeek R1",
        provider: "DeepInfra",
        dialect: "deepseek",
        capabilities: expect.arrayContaining([
          "chat",
          "completion",
          "function_calling",
          "structured_output",
          "thinking",
        ]),
      });
    });

    it("should return 404 if model is not found", async () => {
      const nonExistentId = "non-existent/model-id";
      const response = await SELF.fetch(`http://local.test/api/v1/models/${nonExistentId}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json();

      expect(response.status).toBe(404);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Not Found");
    });

    it("should handle URL-encoded model IDs", async () => {
      const modelId = encodeURIComponent("openai/o4-mini");

      const response = await SELF.fetch(`http://local.test/api/v1/models/${modelId}`, {
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });
      const body = await response.json<{ success: boolean; result: any }>();

      expect(response.status).toBe(200);
      expect(body.success).toBe(true);
      expect(body.result.id).toBe("openai/o4-mini");
    });

    it("should return 401 without authorization", async () => {
      const modelId = "openai/gpt-4.1";
      const response = await SELF.fetch(`http://local.test/api/v1/models/${modelId}`);
      const body = await response.json();

      expect(response.status).toBe(401);
      expect(body.success).toBe(false);
      expect(body.errors[0].message).toBe("Unauthorized");
    });
  });

  // Test that models cannot be modified
  describe("Model API read-only verification", () => {
    it("should not support POST /api/v1/models", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/models`, {
        method: "POST",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          id: "test/model",
          name: "Test Model",
          provider: "Test",
        }),
      });

      // Should return 404 or 405
      expect([404, 405]).toContain(response.status);
    });

    it("should not support PUT /api/v1/models/{id}", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/models/openai/gpt-4.1`, {
        method: "PUT",
        headers: {
          Authorization: "Bearer test-api-key",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({
          description: "Updated description",
        }),
      });

      // Should return 404 or 405
      expect([404, 405]).toContain(response.status);
    });

    it("should not support DELETE /api/v1/models/{id}", async () => {
      const response = await SELF.fetch(`http://local.test/api/v1/models/openai/gpt-4.1`, {
        method: "DELETE",
        headers: {
          Authorization: "Bearer test-api-key",
        },
      });

      // Should return 404 or 405
      expect([404, 405]).toContain(response.status);
    });
  });
});
