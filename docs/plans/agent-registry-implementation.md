# Agent Registry Implementation Specification

## Overview
This document provides the implementation specification for the Agent Registry API using Chanfana's D1AutoEndpoint pattern with Hono.

## Key Understanding: D1AutoEndpoint

D1AutoEndpoint **DOES**:
- Generate SQL queries automatically for CRUD operations
- Handle pagination, filtering, sorting
- Validate request data against Zod schemas
- Provide lifecycle hooks (before/after)

D1AutoEndpoint **DOES NOT**:
- Generate schemas from database tables
- We must provide Zod schemas that match our database structure

## Implementation Structure

### 1. Update wrangler.jsonc

```jsonc
{
  "$schema": "node_modules/wrangler/config-schema.json",
  "name": "agent-registry-api",
  "main": "src/index.ts",
  "compatibility_date": "2025-07-25",
  "observability": {
    "enabled": true
  },
  "d1_databases": [
    {
      "binding": "DB",
      "database_name": "agent-registry-api",
      "database_id": "5eb7b6ae-cb0a-4c8b-9c8b-84d3b208597c"
    }
  ],
  "vars": {
    "API_KEY": "test-api-key"  // Development key
  }
  // Production API_KEY should be set via wrangler secret
}
```

### 2. Minimal Type Definitions (`src/types/index.ts`)

```typescript
import { z } from 'zod';

// Simple JSON array transformer for D1
const jsonArray = z.string().transform((val) => {
  try {
    return JSON.parse(val);
  } catch {
    return [];
  }
});

// Keep schemas minimal - let D1AutoEndpoint handle most validation
export const OrganizationSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().nullable(),
  slug: z.string(),
  createdAt: z.string(),
  updatedAt: z.string()
});

export const ProjectSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().nullable(),
  slug: z.string(),
  organizationId: z.string(),
  createdAt: z.string(),
  updatedAt: z.string()
});

export const UserSchema = z.object({
  id: z.string(),
  name: z.string(),
  email: z.string(),
  organizations: jsonArray,
  createdAt: z.string(),
  updatedAt: z.string()
});

export const ToolSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().nullable(),
  arguments: z.string().nullable(),
  createdAt: z.string(),
  updatedAt: z.string()
});

export const ModelProviderSchema = z.object({
  id: z.string(),
  name: z.string(),
  provider: z.string(),
  description: z.string().nullable(),
  contextSize: z.number(),
  maxTokens: z.number(),
  temperature: z.number().nullable(),
  topP: z.number().nullable(),
  frequencyPenalty: z.number().nullable(),
  presencePenalty: z.number().nullable(),
  capabilities: jsonArray,
  inputModalities: jsonArray,
  outputModalities: jsonArray,
  dialect: z.string().nullable()
});

export const AgentSchema = z.object({
  id: z.string(),
  name: z.string(),
  description: z.string().nullable(),
  model: z.string(),
  instructions: z.string(),
  availableTools: jsonArray.nullable(),
  orgId: z.string(),
  projectId: z.string(),
  createdBy: z.string(),
  updatedBy: z.string(),
  createdAt: z.string(),
  updatedAt: z.string()
});

// Environment type
export interface Env {
  DB: D1Database;
  API_KEY: string;
}
```

### 3. Simple Auth Middleware (`src/middleware/auth.ts`)

```typescript
import { MiddlewareHandler } from 'hono';
import { HTTPException } from 'hono/http-exception';

export const bearerAuth: MiddlewareHandler = async (c, next) => {
  const auth = c.req.header('Authorization');

  if (!auth?.startsWith('Bearer ')) {
    throw new HTTPException(401, { message: 'Unauthorized' });
  }

  const token = auth.substring(7);
  if (token !== c.env.API_KEY) {
    throw new HTTPException(401, { message: 'Invalid API key' });
  }

  // Set test user context
  c.set('auth', {
    userId: '01983d84-2e1d-747d-8e58-fdeb3f5d241d',
    organizations: ['01983d7a-1baa-7069-842a-6815dbfaf38b']
  });

  await next();
};
```

### 4. Domain Services Layer (`src/lib/services/`)

**STRICT RULE**: ALL database queries that are not handled by D1AutoEndpoint MUST be in domain services. NO exceptions.

#### Organization Service (`src/lib/services/organizations.ts`)
```typescript
export class OrganizationService {
  constructor(private db: D1Database) {}

  async checkSlugExists(slug: string): Promise<boolean> {
    const result = await this.db
      .prepare('SELECT 1 FROM organizations WHERE slug = ? LIMIT 1')
      .bind(slug)
      .first();
    return !!result;
  }

  async hasProjects(orgId: string): Promise<boolean> {
    const result = await this.db
      .prepare('SELECT 1 FROM projects WHERE organizationId = ? LIMIT 1')
      .bind(orgId)
      .first();
    return !!result;
  }

  async findByIds(ids: string[]) {
    if (!ids.length) return { results: [] };

    const placeholders = ids.map(() => '?').join(',');
    return await this.db
      .prepare(`SELECT * FROM organizations WHERE id IN (${placeholders})`)
      .bind(...ids)
      .all();
  }
}
```

#### Project Service (`src/lib/services/projects.ts`)
```typescript
export class ProjectService {
  constructor(private db: D1Database) {}

  async checkSlugExists(slug: string): Promise<boolean> {
    const result = await this.db
      .prepare('SELECT 1 FROM projects WHERE slug = ? LIMIT 1')
      .bind(slug)
      .first();
    return !!result;
  }

  async belongsToOrganization(projectId: string, orgId: string): Promise<boolean> {
    const result = await this.db
      .prepare('SELECT 1 FROM projects WHERE id = ? AND organizationId = ? LIMIT 1')
      .bind(projectId, orgId)
      .first();
    return !!result;
  }
}
```

#### Model Service (`src/lib/services/models.ts`)
```typescript
export class ModelService {
  constructor(private db: D1Database) {}

  async exists(modelId: string): Promise<boolean> {
    const result = await this.db
      .prepare('SELECT 1 FROM model_providers WHERE id = ? LIMIT 1')
      .bind(modelId)
      .first();
    return !!result;
  }
}
```

#### Tool Service (`src/lib/services/tools.ts`)
```typescript
export class ToolService {
  constructor(private db: D1Database) {}

  async validateIds(toolIds: string[]): Promise<boolean> {
    if (!toolIds.length) return true;

    const placeholders = toolIds.map(() => '?').join(',');
    const result = await this.db
      .prepare(`SELECT COUNT(*) as count FROM tools WHERE id IN (${placeholders})`)
      .bind(...toolIds)
      .first();

    return result?.count === toolIds.length;
  }
}
```

#### Agent Service (`src/lib/services/agents.ts`)
```typescript
export class AgentService {
  constructor(private db: D1Database) {}

  async findByUserOrgs(orgIds: string[], filters?: {
    orgId?: string;
    projectId?: string;
  }) {
    let query = 'SELECT * FROM agents WHERE orgId IN (';
    const placeholders = orgIds.map(() => '?').join(',');
    query += placeholders + ')';

    const params = [...orgIds];

    if (filters?.orgId) {
      query += ' AND orgId = ?';
      params.push(filters.orgId);
    }

    if (filters?.projectId) {
      query += ' AND projectId = ?';
      params.push(filters.projectId);
    }

    return await this.db
      .prepare(query)
      .bind(...params)
      .all();
  }

  async belongsToOrg(agentId: string, orgId: string): Promise<boolean> {
    const result = await this.db
      .prepare('SELECT 1 FROM agents WHERE id = ? AND orgId = ? LIMIT 1')
      .bind(agentId, orgId)
      .first();
    return !!result;
  }
}
```

### 5. Organization Endpoints (`src/endpoints/organizations.ts`)

```typescript
import { D1CreateEndpoint, D1ReadEndpoint, D1UpdateEndpoint,
         D1DeleteEndpoint, D1ListEndpoint } from 'chanfana';
import { HTTPException } from 'hono/http-exception';
import { OrganizationSchema } from '../types';
import { OrganizationService } from '../lib/services/organizations';
import { v7 as uuidv7 } from 'uuid';

const orgMeta = {
  model: {
    schema: OrganizationSchema,
    primaryKeys: ['id'],
    tableName: 'organizations'
  }
};

export class CreateOrganization extends D1CreateEndpoint {
  _meta = orgMeta;

  async before(c) {
    const data = await this.getValidatedData();
    data.body.id = uuidv7();

    // ALL database queries through service
    const service = new OrganizationService(c.env.DB);
    if (await service.checkSlugExists(data.body.slug)) {
      throw new HTTPException(400, { message: 'Slug already exists' });
    }
  }
}

export class GetOrganization extends D1ReadEndpoint {
  _meta = orgMeta;

  async before(c) {
    const auth = c.get('auth');
    const { id } = c.req.param();

    if (!auth.organizations.includes(id)) {
      throw new HTTPException(403, { message: 'Access denied' });
    }
  }
}

export class UpdateOrganization extends D1UpdateEndpoint {
  _meta = orgMeta;

  async before(c) {
    const auth = c.get('auth');
    const { id } = c.req.param();

    if (!auth.organizations.includes(id)) {
      throw new HTTPException(403, { message: 'Access denied' });
    }
  }
}

export class DeleteOrganization extends D1DeleteEndpoint {
  _meta = orgMeta;

  async before(c) {
    const { id } = c.req.param();

    // ALL database queries through service
    const service = new OrganizationService(c.env.DB);
    if (await service.hasProjects(id)) {
      throw new HTTPException(400, {
        message: 'Cannot delete organization with existing projects'
      });
    }
  }
}

export class ListOrganizations extends D1ListEndpoint {
  _meta = {
    ...orgMeta,
  };
  filterFields = ['name', 'slug'];

  // Override list to filter by user's organizations
  async list(c) {
    const auth = c.get('auth');

    // ALL database queries through service
    const service = new OrganizationService(c.env.DB);
    return await service.findByIds(auth.organizations);
  }
}
```

### 6. Agent Endpoints with Validation (`src/endpoints/agents.ts`)

```typescript
import { D1CreateEndpoint, D1ReadEndpoint, D1UpdateEndpoint,
         D1DeleteEndpoint, D1ListEndpoint, contentJson } from 'chanfana';
import { HTTPException } from 'hono/http-exception';
import { AgentSchema } from '../types';
import { ValidationService } from '../lib/services/validation';
import { v7 as uuidv7 } from 'uuid';

const agentMeta = {
  model: {
    schema: AgentSchema,
    primaryKeys: ['id'],
    tableName: 'agents'
  }
};

export class CreateAgent extends D1CreateEndpoint {
  _meta = agentMeta;

  // Custom schema to exclude auto-generated fields
  schema = {
    request: {
      body: contentJson(
        AgentSchema.pick({
          name: true,
          description: true,
          model: true,
          instructions: true,
          availableTools: true,
          orgId: true,
          projectId: true
        })
      )
    }
  };

  async before(c) {
    const data = await this.getValidatedData();
    const auth = c.get('auth');
    const service = new ValidationService(c.env.DB);

    // Set generated fields
    data.body.id = uuidv7();
    data.body.createdBy = auth.userId;
    data.body.updatedBy = auth.userId;

    // Validate access
    if (!auth.organizations.includes(data.body.orgId)) {
      throw new HTTPException(403, { message: 'Access denied to organization' });
    }

    // Validate project belongs to org - using service
    const projectService = new ProjectService(c.env.DB);
    if (!await projectService.belongsToOrganization(data.body.projectId, data.body.orgId)) {
      throw new HTTPException(400, { message: 'Project does not belong to organization' });
    }

    // Validate model exists - using service
    const modelService = new ModelService(c.env.DB);
    if (!await modelService.exists(data.body.model)) {
      throw new HTTPException(400, { message: 'Invalid model ID' });
    }

    // Validate and stringify tools
    if (data.body.availableTools?.length) {
      if (!await service.validateToolIds(data.body.availableTools)) {
        throw new HTTPException(400, { message: 'Invalid tool IDs' });
      }
      data.body.availableTools = JSON.stringify(data.body.availableTools);
    }
  }
}

export class ListAgents extends D1ListEndpoint {
  _meta = {
    ...agentMeta,
  };
  filterFields = ['name', 'model', 'orgId', 'projectId'];
  searchFields = ['name', 'description'];

  async list(c) {
    const auth = c.get('auth');
    const { orgId, projectId } = c.req.query();

    // ALL database queries through service
    const service = new AgentService(c.env.DB);
    const filters: any = {};

    if (orgId && auth.organizations.includes(orgId)) {
      filters.orgId = orgId;
    }

    if (projectId) {
      filters.projectId = projectId;
    }

    return await service.findByUserOrgs(auth.organizations, filters);
  }
}
```

### 7. Main App Setup (`src/index.ts`)

```typescript
import { fromHono } from 'chanfana';
import { Hono } from 'hono';
import { bearerAuth } from './middleware/auth';

// Import endpoints
import * as organizations from './endpoints/organizations';
import * as projects from './endpoints/projects';
import * as users from './endpoints/users';
import * as tools from './endpoints/tools';
import * as agents from './endpoints/agents';
import * as models from './endpoints/models';

const app = new Hono();

// API v1 router with auth
const v1 = new Hono();
v1.use('*', bearerAuth);

// Setup OpenAPI
const openapi = fromHono(v1, {
  docs_url: '/openapi.json',
  schema: {
    info: {
      title: 'Agent Registry API',
      version: '1.0.0'
    },
    security: [{ bearerAuth: [] }],
    components: {
      securitySchemes: {
        bearerAuth: {
          type: 'http',
          scheme: 'bearer'
        }
      }
    }
  }
});

// Register endpoints
// Organizations
openapi.post('/organizations', organizations.CreateOrganization);
openapi.get('/organizations', organizations.ListOrganizations);
openapi.get('/organizations/:id', organizations.GetOrganization);
openapi.put('/organizations/:id', organizations.UpdateOrganization);
openapi.delete('/organizations/:id', organizations.DeleteOrganization);

// Similar registration for other entities...

// Mount API
app.route('/api/v1', v1);

// Health check
app.get('/health', (c) => c.json({ status: 'ok' }));

export default app;
```

## Key Simplifications

1. **No complex type system** - Let D1AutoEndpoint handle most validation
2. **Minimal service layer** - Only for queries D1AutoEndpoint can't handle
3. **Simple auth** - Just API key validation for now
4. **Let D1AutoEndpoint do the work** - It handles all basic CRUD SQL generation

## Development Steps

1. Install dependencies:
   ```bash
   bun add uuid zod hono chanfana
   bun add -d vitest @vitest/ui miniflare wrangler
   ```
2. Run `wrangler types` to generate D1Database types
3. Set up test configuration in `vitest.config.ts`
4. Create test utilities and database mocking
5. Start with Organizations (simplest entity) and its tests
6. Run development server in background:
   ```bash
   # IMPORTANT: Always run dev server in background to keep terminal available
   bun run dev &
   # Or use a separate terminal/tmux session

   # To stop the background process later:
   # jobs  # to see background jobs
   # kill %1  # to stop job #1
   ```
7. Test with: `bun test` (in main terminal)
8. Add other entities incrementally with full test coverage
9. Run full test suite before committing

## Testing Strategy

### Unit Tests

Each service and endpoint should have comprehensive unit tests. Use vitest for testing.

#### Service Layer Tests (`src/lib/services/__tests__/`)

```typescript
// src/lib/services/__tests__/organizations.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import { OrganizationService } from '../organizations';
import { createTestDatabase } from '../../test-utils';

describe('OrganizationService', () => {
  let db: D1Database;
  let service: OrganizationService;

  beforeEach(async () => {
    db = await createTestDatabase();
    service = new OrganizationService(db);
  });

  describe('checkSlugExists', () => {
    it('should return true when slug exists', async () => {
      await db.prepare('INSERT INTO organizations (id, name, slug) VALUES (?, ?, ?)')
        .bind('test-id', 'Test Org', 'test-slug')
        .run();

      const exists = await service.checkSlugExists('test-slug');
      expect(exists).toBe(true);
    });

    it('should return false when slug does not exist', async () => {
      const exists = await service.checkSlugExists('non-existent');
      expect(exists).toBe(false);
    });
  });

  describe('hasProjects', () => {
    it('should return true when organization has projects', async () => {
      // Test implementation
    });
  });
});
```

#### Endpoint Tests (`src/endpoints/__tests__/`)

```typescript
// src/endpoints/__tests__/organizations.test.ts
import { describe, it, expect, beforeEach } from 'vitest';
import { CreateOrganization } from '../organizations';
import { createMockContext } from '../../test-utils';

describe('CreateOrganization', () => {
  it('should create organization with valid data', async () => {
    const ctx = createMockContext({
      body: { name: 'Test Org', slug: 'test-org' },
      auth: { userId: 'test-user', organizations: [] }
    });

    const endpoint = new CreateOrganization();
    await endpoint.handler(ctx);

    expect(ctx.json).toHaveBeenCalledWith(
      expect.objectContaining({
        id: expect.any(String),
        name: 'Test Org',
        slug: 'test-org'
      })
    );
  });

  it('should reject duplicate slugs', async () => {
    // Test implementation
  });
});
```

### Integration Tests (`tests/`)

Full API integration tests that test the entire request/response cycle:

```typescript
// tests/api/organizations.test.ts
import { describe, it, expect, beforeAll, afterAll } from 'vitest';
import { unstable_dev } from 'wrangler';
import type { UnstableDevWorker } from 'wrangler';

describe('Organizations API', () => {
  let worker: UnstableDevWorker;

  beforeAll(async () => {
    worker = await unstable_dev('src/index.ts', {
      experimental: { disableExperimentalWarning: true },
      vars: { API_KEY: 'test-key' }
    });
  });

  afterAll(async () => {
    await worker.stop();
  });

  it('should create and retrieve organization', async () => {
    // Create organization
    const createRes = await worker.fetch('/api/v1/organizations', {
      method: 'POST',
      headers: {
        'Authorization': 'Bearer test-key',
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({
        name: 'Test Organization',
        slug: 'test-org',
        description: 'Test description'
      })
    });

    expect(createRes.status).toBe(201);
    const created = await createRes.json();
    expect(created).toHaveProperty('id');
    expect(created.name).toBe('Test Organization');

    // Retrieve organization
    const getRes = await worker.fetch(`/api/v1/organizations/${created.id}`, {
      headers: { 'Authorization': 'Bearer test-key' }
    });

    expect(getRes.status).toBe(200);
    const retrieved = await getRes.json();
    expect(retrieved).toEqual(created);
  });

  it('should enforce authorization', async () => {
    const res = await worker.fetch('/api/v1/organizations', {
      method: 'GET'
    });
    expect(res.status).toBe(401);
  });

  it('should validate required fields', async () => {
    const res = await worker.fetch('/api/v1/organizations', {
      method: 'POST',
      headers: {
        'Authorization': 'Bearer test-key',
        'Content-Type': 'application/json'
      },
      body: JSON.stringify({ name: 'Missing Slug' })
    });

    expect(res.status).toBe(400);
    const error = await res.json();
    expect(error.message).toContain('slug');
  });
});
```

### Test Utilities (`src/test-utils.ts`)

```typescript
import { D1Database } from '@cloudflare/workers-types';
import { getMiniflareBindings } from 'miniflare';

export async function createTestDatabase(): Promise<D1Database> {
  const { DB } = getMiniflareBindings();

  // Run migrations
  const migrations = await readdir('migrations');
  for (const migration of migrations.sort()) {
    const sql = await readFile(`migrations/${migration}`, 'utf-8');
    await DB.exec(sql);
  }

  return DB;
}

export function createMockContext(options: {
  body?: any;
  params?: Record<string, string>;
  query?: Record<string, string>;
  auth?: { userId: string; organizations: string[] };
}) {
  // Mock implementation
}
```

### Manual Testing (for development)

```bash
# Create an organization
curl -X POST http://localhost:8787/api/v1/organizations \
  -H "Authorization: Bearer test-api-key" \
  -H "Content-Type: application/json" \
  -d '{"name": "Test Org", "slug": "test-org"}'

# List organizations
curl http://localhost:8787/api/v1/organizations \
  -H "Authorization: Bearer test-api-key"
```

### Testing Commands

```bash
# Run all tests
bun test

# Run unit tests only
bun test src/

# Run integration tests only
bun test tests/

# Run with coverage
bun test --coverage

# Run in watch mode
bun test --watch
```

## Success Criteria
- [ ] All entities use D1AutoEndpoint classes correctly
- [ ] Bearer token auth works on all endpoints
- [ ] JSON fields properly transformed
- [ ] Foreign key constraints validated
- [ ] Permissions properly enforced
- [ ] Service layer handles ALL non-D1AutoEndpoint queries (NO exceptions)
- [ ] OpenAPI spec generated at /api/v1/openapi.json
- [ ] All CRUD operations functional
- [ ] Unit tests for all service layer methods with >90% coverage
- [ ] Unit tests for all endpoint lifecycle hooks and validations
- [ ] Integration tests for complete API workflows
- [ ] Tests verify error handling and edge cases
- [ ] Test utilities properly mock D1 database
- [ ] All tests pass in CI/CD pipeline
