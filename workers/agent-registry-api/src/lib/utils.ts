import { compare } from "semver";
import slug from "slug";

// Extend slug with custom character mappings for common symbols
slug.extend({
  "@": "-at-",
  "#": "-hash-",
  _: "-",
  "&": "-and-",
  "+": "-plus-",
});

/**
 * Generates a URL-friendly slug from a given string using the slug library
 * @param name - The string to convert to a slug
 * @returns A URL-friendly slug
 */
export function generateSlug(name: string): string {
  if (!name || name.trim().length === 0) {
    throw new Error("Name cannot be empty");
  }

  // Generate slug and clean up multiple consecutive dashes
  return slug(name);
}

/**
 * Validates a slug against the required format
 * @param slug - The slug to validate
 * @returns True if valid, false otherwise
 */
export function isValidSlug(slug: string): boolean {
  return /^[a-z0-9-_]{3,}$/.test(slug);
}

/**
 * Compare two semantic version strings using the semver library
 * Returns: -1 if versionA < versionB, 0 if equal, 1 if versionA > versionB
 */
export function compareSemVer(versionA: string, versionB: string): number {
  return compare(versionA, versionB);
}
