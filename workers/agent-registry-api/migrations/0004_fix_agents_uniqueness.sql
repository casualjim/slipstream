-- Adjust agents uniqueness to allow multiple versions per slug within org/project
-- 1) Drop existing unique index on (slug, organization, project)
-- 2) Add a composite unique constraint on (slug, version, organization, project)
-- 3) Add helpful index for lookups

-- SQLite note:
-- We can't drop a UNIQUE constraint inline; we need to drop the index enforcing it.
-- The index name is auto-generated unless explicitly named. Since 0001 used "UNIQUE (slug, organization, project)"
-- SQLite will create an implicit index. We'll recreate the table schema via standard ALTER TABLE sequence.



-- Create a new table with corrected constraints
CREATE TABLE agents_new (
  slug TEXT NOT NULL CHECK (length(slug) >= 3 AND slug GLOB '[A-Za-z0-9-_]*'),
  version TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  model TEXT NOT NULL,
  instructions TEXT NOT NULL,
  availableTools TEXT,
  organization TEXT NOT NULL,
  project TEXT NOT NULL,
  createdBy TEXT NOT NULL,
  updatedBy TEXT NOT NULL,
  createdAt DATETIME DEFAULT CURRENT_TIMESTAMP,
  updatedAt DATETIME DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (slug, version),
  FOREIGN KEY (organization) REFERENCES organizations (slug) ON DELETE CASCADE,
  FOREIGN KEY (project) REFERENCES projects (slug) ON DELETE CASCADE,
  FOREIGN KEY (createdBy) REFERENCES users (id) ON DELETE CASCADE,
  FOREIGN KEY (updatedBy) REFERENCES users (id) ON DELETE CASCADE,
  FOREIGN KEY (model) REFERENCES model_providers (id),
  UNIQUE (slug, version, organization, project)
);

-- Copy all existing data
INSERT INTO agents_new (
  slug, version, name, description, model, instructions, availableTools,
  organization, project, createdBy, updatedBy, createdAt, updatedAt
)
SELECT
  slug, version, name, description, model, instructions, availableTools,
  organization, project, createdBy, updatedBy, createdAt, updatedAt
FROM agents;

-- Replace old table
DROP TABLE agents;
ALTER TABLE agents_new RENAME TO agents;

-- Recreate indexes lost during table replacement
CREATE INDEX idx_agents_organization ON agents (organization);
CREATE INDEX idx_agents_project ON agents (project);
CREATE INDEX idx_agents_createdBy ON agents (createdBy);
CREATE INDEX idx_agents_slug_version ON agents (slug, version);
CREATE INDEX idx_agents_model ON agents (model);

-- Helpful index for latest lookups
CREATE INDEX idx_agents_slug_org_proj_createdAt ON agents (slug, organization, project, createdAt);


