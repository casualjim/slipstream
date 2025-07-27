-- Create organizations table with slug as primary key
CREATE TABLE organizations (
  slug TEXT PRIMARY KEY CHECK (length(slug) >= 3 AND slug GLOB '[A-Za-z0-9-_]*'),
  name TEXT NOT NULL,
  description TEXT,
  createdAt DATETIME DEFAULT CURRENT_TIMESTAMP,
  updatedAt DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Create projects table with slug as primary key
CREATE TABLE projects (
  slug TEXT PRIMARY KEY CHECK (length(slug) >= 3 AND slug GLOB '[A-Za-z0-9-_]*'),
  name TEXT NOT NULL,
  description TEXT,
  createdAt DATETIME DEFAULT CURRENT_TIMESTAMP,
  updatedAt DATETIME DEFAULT CURRENT_TIMESTAMP,
  organization TEXT NOT NULL,
  FOREIGN KEY (organization) REFERENCES organizations (slug) ON DELETE CASCADE,
  UNIQUE (slug, organization)
);

-- Create users table (keeps id as primary key)
CREATE TABLE users (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  email TEXT NOT NULL UNIQUE,
  createdAt DATETIME DEFAULT CURRENT_TIMESTAMP,
  updatedAt DATETIME DEFAULT CURRENT_TIMESTAMP,
  organizations TEXT NOT NULL -- JSON array of organization slugs
);

-- Create model_providers table (keeps id as primary key since these are external references)
CREATE TABLE model_providers (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  provider TEXT NOT NULL CHECK (provider IN ('OpenAI', 'Anthropic', 'DeepInfra', 'Google', 'OpenRouter')),
  description TEXT,
  contextSize INTEGER NOT NULL,
  maxTokens INTEGER,
  temperature REAL,
  topP REAL,
  frequencyPenalty REAL,
  presencePenalty REAL,
  capabilities TEXT NOT NULL, -- JSON array
  inputModalities TEXT NOT NULL, -- JSON array
  outputModalities TEXT NOT NULL, -- JSON array
  dialect TEXT CHECK (dialect IS NULL OR dialect IN ('openai', 'anthropic', 'deepseek'))
);

-- Create tools table with composite primary key (slug, version, provider)
CREATE TABLE tools (
  slug TEXT NOT NULL CHECK (length(slug) >= 3 AND slug GLOB '[A-Za-z0-9-_]*'),
  version TEXT NOT NULL,
  provider TEXT NOT NULL CHECK (provider IN ('Client', 'Local', 'MCP', 'Restate')),
  name TEXT NOT NULL,
  description TEXT,
  arguments TEXT, -- JSON Schema object
  createdAt DATETIME DEFAULT CURRENT_TIMESTAMP,
  updatedAt DATETIME DEFAULT CURRENT_TIMESTAMP,
  PRIMARY KEY (slug, version, provider),
  UNIQUE (name, version)
);

-- Create agents table with composite primary key (slug, version)
CREATE TABLE agents (
  slug TEXT NOT NULL CHECK (length(slug) >= 3 AND slug GLOB '[A-Za-z0-9-_]*'),
  version TEXT NOT NULL,
  name TEXT NOT NULL,
  description TEXT,
  model TEXT NOT NULL,
  instructions TEXT NOT NULL,
  availableTools TEXT, -- JSON array of tool references {slug, version}
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

-- Create indexes for better performance
CREATE INDEX idx_projects_organization ON projects (organization);

CREATE INDEX idx_agents_organization ON agents (organization);

CREATE INDEX idx_agents_project ON agents (project);

CREATE INDEX idx_agents_createdBy ON agents (createdBy);

CREATE INDEX idx_agents_slug_version ON agents (slug, version);

CREATE INDEX idx_tools_slug_version_provider ON tools (slug, version, provider);

CREATE INDEX idx_users_email ON users (email);
