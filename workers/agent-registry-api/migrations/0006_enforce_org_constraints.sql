-- Enforce unique slugs for organizations and prevent deleting organizations that still have projects.
-- This migration is additive and assumes existing tables `organizations` and `projects`.

-- 1) Ensure organizations.slug is unique
-- If an index with same name exists, Cloudflare D1 ignores duplicate creation,
-- but we defensively use IF NOT EXISTS where supported.
CREATE UNIQUE INDEX IF NOT EXISTS idx_organizations_slug_unique ON organizations(slug);

-- 2) Add a foreign key from projects.organization to organizations.slug to restrict deletes
-- Cloudflare D1 (SQLite) requires foreign_keys pragma enabled at runtime; Workers runtime typically enables it.
-- We create the constraint via a trigger pattern because altering existing tables to add FK constraints is limited.

-- Create a trigger to prevent deleting organizations that have projects
CREATE TRIGGER IF NOT EXISTS trg_organizations_prevent_delete_when_projects
BEFORE DELETE ON organizations
FOR EACH ROW
BEGIN
  SELECT CASE
    WHEN EXISTS (
      SELECT 1 FROM projects WHERE organization = OLD.slug LIMIT 1
    )
    THEN RAISE(ABORT, 'Cannot delete organization with existing projects')
  END;
END;
