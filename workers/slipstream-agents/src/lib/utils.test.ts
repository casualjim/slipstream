import { describe, expect, it } from "vitest";
import { generateSlug } from "./utils";

describe("generateSlug", () => {
  it("should convert basic names to slugs", () => {
    expect(generateSlug("My Tool")).toBe("my-tool");
    expect(generateSlug("Weather API")).toBe("weather-api");
    expect(generateSlug("Data Fetcher")).toBe("data-fetcher");
  });

  it("should handle special characters", () => {
    expect(generateSlug("Tool@Home")).toBe("tool-at-home");
    expect(generateSlug("API-Tool")).toBe("api-tool");
    expect(generateSlug("Tool_Name")).toBe("tool-name");
    expect(generateSlug("Tool#1")).toBe("tool-hash-1");
  });

  it("should handle multiple spaces and special chars", () => {
    expect(generateSlug("My   Special   Tool")).toBe("my-special-tool");
    expect(generateSlug("Tool---Name")).toBe("tool-name");
    expect(generateSlug("Tool___Name")).toBe("tool-name");
  });

  it("should handle leading/trailing spaces and special chars", () => {
    expect(generateSlug("  My Tool  ")).toBe("my-tool");
    expect(generateSlug("--My Tool--")).toBe("my-tool");
    expect(generateSlug("__My Tool__")).toBe("my-tool");
  });

  it("should ensure minimum length", () => {
    // These should now be caught by schema validation before reaching generateSlug
    expect(generateSlug("ABC")).toBe("abc");
    expect(generateSlug("ABCD")).toBe("abcd");
    expect(generateSlug("Test123")).toBe("test123");
  });

  it("should handle empty or whitespace-only strings", () => {
    expect(() => generateSlug("")).toThrow("Name cannot be empty");
    expect(() => generateSlug("   ")).toThrow("Name cannot be empty");
  });

  it("should handle mixed case", () => {
    expect(generateSlug("MyAPI Tool")).toBe("myapi-tool");
    expect(generateSlug("HTTP Client")).toBe("http-client");
  });

  it("should handle numbers", () => {
    expect(generateSlug("Tool 123")).toBe("tool-123");
    expect(generateSlug("API v2")).toBe("api-v2");
  });
});
