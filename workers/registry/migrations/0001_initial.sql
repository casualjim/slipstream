-- Migration 0001: Create agents table
DROP TABLE IF EXISTS agents;

CREATE TABLE agents (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    description TEXT,
    version TEXT NOT NULL DEFAULT '1.0.0',
    author TEXT,
    tags TEXT, -- JSON array as text
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Create index for faster lookups
CREATE INDEX idx_agents_name ON agents(name);
CREATE INDEX idx_agents_author ON agents(author);

-- Seed data
INSERT INTO agents (name, description, version, author, tags) VALUES
    ('code-reviewer', 'Automated code review agent that checks for best practices and security issues', '1.0.0', 'Slipstream Team', '["code-review", "security", "analysis"]'),
    ('documentation-generator', 'Generates comprehensive documentation from code comments and structure', '1.2.1', 'Slipstream Team', '["documentation", "automation", "productivity"]'),
    ('test-generator', 'Creates unit tests and integration tests for existing code', '2.0.0', 'Slipstream Team', '["testing", "automation", "quality"]'),
    ('deployment-assistant', 'Helps configure CI/CD pipelines and deployment strategies', '1.5.0', 'DevOps Team', '["deployment", "ci-cd", "automation"]'),
    ('performance-analyzer', 'Analyzes code performance and suggests optimizations', '1.1.0', 'Performance Team', '["performance", "optimization", "analysis"]');
