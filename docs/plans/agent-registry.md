# Agent Registry API Implementation

## Version Handling Overview

For both Agents and Tools:

Reads
- GET and HAS support versionless lookups. If you omit the `version` in the reference, the registry returns/checks the latest version:
  - Agents: latest stored at key "slug"
  - Tools: latest stored at key "provider/slug"

Writes
- PUT requires a version and will:
  - Create/update the versioned record ("slug/version" for agents, "provider/slug/version" for tools)
  - Update the latest pointer to the submitted subject
- DEL requires a version and will:
  - Delete the specified versioned record
  - If that version was the current latest, recompute latest from remaining versions (highest semantic version), or remove latest pointer if none remain

References and definitions
- AgentRef.version: Option<String>
- ToolRef.version: Option<String>
- AgentDefinition.version and ToolDefinition.version remain required

Reference implementations
- HTTP: [`crates/slipstream-metadata/src/http/http_agent.rs`](../../crates/slipstream-metadata/src/http/http_agent.rs:1), [`crates/slipstream-metadata/src/http/http_tool.rs`](../../crates/slipstream-metadata/src/http/http_tool.rs:1)
- Memory: [`crates/slipstream-metadata/src/memory/memory_agent.rs`](crates/slipstream-metadata/src/memory/memory_agent.rs:1), [`crates/slipstream-metadata/src/memory/memory_tool.rs`](crates/slipstream-metadata/src/memory/memory_tool.rs:1)
- NATS: [`crates/slipstream-metadata/src/nats/agent.rs`](crates/slipstream-metadata/src/nats/agent.rs:1), [`crates/slipstream-metadata/src/nats/tool.rs`](crates/slipstream-metadata/src/nats/tool.rs:1)

Developer guidance
- Omit version in AgentRef/ToolRef only for reads when “latest” is acceptable
- Always include version for create/update/delete flows
- Ensure tests cover both versioned and versionless paths


## Objective
Create an agent registry API using a Hono API server with [Chanfana](https://chanfana.pages.dev/introduction) for OpenAPI support and Cloudflare D1 integration via auto endpoints (https://chanfana.pages.dev/endpoints/auto/d1). Define data structures to store agent configurations.

All endpoints must implement bearer token authentication using API keys for secure access.

---

## Data Structures
Use the following TypeScript interfaces for all schemas. Ensure validation and constraints are enforced in the via the D1AutoEndpoint functionality with chafana and a strict database schema that has exhaustive constraints.

You MUST read the d1 auto endpoints docs from chanfana: https://chanfana.pages.dev/endpoints/auto/d1

### Agent
```ts
import { v7 as uuid7 } from "uuid";

export interface Agent {
  id: string; // uuid7, readonly
  name: string; // required
  description?: string; // optional
  model: string; // required, must be a known model ID
  instructions: string; // required
  availableTools?: Tool[]; // optional
  version: string; // required
  slug: string; // required [A-Za-z0-9-_]{3,}
  organization: string; // uuid7, readonly, must be valid organization, user must belong to it
  project: string; // uuid7, readonly, must be valid project in accessible organization
  createdBy: string; // uuid7, readonly, valid userId
  updatedBy: string; // uuid7, readonly, valid userId
}
```

### Tool
```ts
export enum ToolProvider {
  Client,
  Local,
  MCP,
  Restate
}

export interface Tool {
  id: string; // uuid7, readonly
  slug: string; // required [A-Za-z0-9-_]{3,}
  name: string; // required
  description?: string; // optional
  arguments?: object; // valid JSON Schema if present
  version: string;
  provider: ToolProvider; // required
}
```

### Organization

```ts
export const WAGYU_ORGANIZATION_ID = "01983d7a-1baa-7069-842a-6815dbfaf38b";

export interface Organization {
  id: string; // uuid7, readonly
  name: string; // required
  description?: string; // optional
  createdAt: Date; // readonly
  updatedAt: Date; // readonly
  slug: string; // required [A-Za-z0-9-_]{3,}
}

// for seeding, use these exact values
export const organizations: Organization[] = [
  {
    id: WAGYU_ORGANIZATION_ID,
    name: "Wagyu",
    description: "The family organization",
    createdAt: new Date(2025, 7, 24, 12, 0, 0),
    updatedAt: new Date(2025, 7, 24, 12, 0, 0),
    slug: "wagyu",
  },
];
```

### Project

```ts
export const WAGYU_PROJECT_ID = "01983d7a-930e-72ba-8d1e-6d58f2253e90";

export interface Project {
  id: string; // uuid7, readonly
  name: string; // required
  description?: string; // optional
  createdAt: Date; // readonly
  updatedAt: Date; // readonly
  slug: string; // required [A-Za-z0-9-_]{3,}
  organizationId: string; // uuid7, readonly, reference to valid organization
}

// for seeding, use these exact values
export const projects: Project[] = [
  {
    id: WAGYU_PROJECT_ID,
    name: "Default",
    description: "Default project under the Wagyu organization",
    createdAt: new Date(2025, 7, 24, 12, 0, 0),
    updatedAt: new Date(2025, 7, 24, 12, 0, 0),
    slug: "wagyu-project",
    organizationId: WAGYU_ORGANIZATION_ID,
  },
];
```

### User
| Property        | Type     | Constraints              |
| --------------- | -------- | ------------------------ |
| `id`            | string   | uuid7, required          |
| `name`          | string   | required                 |
| `email`         | string   | required                 |
| `createdAt`     | Date     | required                 |
| `updatedAt`     | Date     | required                 |
| `organizations` | string[] | List of organization IDs |

```ts
export interface User {
  id: string; // uuid7, readonly
  name: string; // required
  email: string; // required
  createdAt: Date; // readonly
  updatedAt: Date; // readonly
  organizations: string[]; // List of organization IDs this user belongs to, required to be present and have valid references
}

// for seeding, use these exact values
export const users: User[] = [
  {
    id: "01983d84-2e1d-747d-8e58-fdeb3f5d241d",
    name: "Test User",
    email: "test.user@example.com",
    createdAt: new Date(2025, 7, 24, 12, 0, 0),
    updatedAt: new Date(2025, 7, 24, 12, 0, 0),
    organizations: [WAGYU_ORGANIZATION_ID], // Reference to the Wagyu organization
  },
];
```

### ModelProvider


```ts
export enum ModelCapabilities {
  CHAT = "chat",
  COMPLETION = "completion",
  EMBEDDINGS = "embeddings",
  RERANKING = "reranking",
  FUNCTION_CALLING = "function_calling",
  STRUCTURED_OUTPUT = "structured_output",
  CODE_EXECUTION = "code_execution",
  SEARCH = "search",
  THINKING = "thinking",
  IMAGE_GENERATION = "image_generation",
  VIDEO_GENERATION = "video_generation",
  CACHING = "caching",
  TUNING = "tuning",
  BATCH = "batch",
}

export enum Modalities {
  TEXT = "text",
  IMAGE = "image",
  VIDEO = "video",
  AUDIO = "audio",
  PDF = "pdf",
}

export enum APIDialect {
  OPENAI = "openai",
  ANTHROPIC = "anthropic",
  DEEPSEEK = "deepseek",
}

export enum Provider {
  OPENAI = "OpenAI",
  ANTHROPIC = "Anthropic",
  DEEPINFRA = "DeepInfra",
  GOOGLE = "Google",
  OPENROUTER = "OpenRouter",
}

export interface ModelConfiguration {
  id: string; // uuid7, readonly
  name: string; // required
  provider: Provider; // required
  description?: string; // optional
  contextSize: number;, // required
  maxTokens?: number; // optional
  temperature?: number; // optional
  topP?: number; // optional
  frequencyPenalty?: number; // optional
  presencePenalty?: number; // optional
  capabilities: ModelCapabilities[]; // required to have at least 1, all entries must be valid
  inputModalities: Modalities[]; // required to have at least 1, all entries must be valid
  outputModalities: Modalities[]; // required to have at least 1, all entries must be valid
  dialect?: APIDialect; // optional
}

export const modelConfigurations: ModelConfiguration[] = [
  {
    id: "deepseek-ai/DeepSeek-R1-0528",
    name: "DeepSeek R1",
    description: "DeepSeek R1 model, optimized for reasoning tasks.",
    provider: Provider.DEEPINFRA,
    dialect: APIDialect.DEEPSEEK,
    capabilities: [
      ModelCapabilities.CHAT,
      ModelCapabilities.COMPLETION,
      ModelCapabilities.FUNCTION_CALLING,
      ModelCapabilities.STRUCTURED_OUTPUT,
      ModelCapabilities.THINKING,
    ],
    outputModalities: [Modalities.TEXT],
    contextSize: 163840,
    maxTokens: 16384,
    inputModalities: [Modalities.TEXT],
  },
  {
    id: "deepseek-ai/DeepSeek-V3-0324",
    name: "DeepSeek V3",
    description: "DeepSeek V3 model, optimized for general tasks.",
    provider: Provider.DEEPINFRA,
    dialect: APIDialect.DEEPSEEK,
    capabilities: [
      ModelCapabilities.CHAT,
      ModelCapabilities.COMPLETION,
      ModelCapabilities.FUNCTION_CALLING,
      ModelCapabilities.STRUCTURED_OUTPUT,
    ],
    inputModalities: [Modalities.TEXT],
    outputModalities: [Modalities.TEXT],
    contextSize: 163840,
    maxTokens: 16384,
  },
  {
    id: "moonshotai/Kimi-K2-Instruct",
    name: "Kimi K2",
    dialect: APIDialect.ANTHROPIC,
    description:
      "Kimi K2 model, optimized for agentic capabilities, including advanced tool use, reasoning, and code synthesis",
    inputModalities: [Modalities.TEXT],
    outputModalities: [Modalities.TEXT],
    provider: Provider.DEEPINFRA,
    capabilities: [
      ModelCapabilities.CHAT,
      ModelCapabilities.COMPLETION,
      ModelCapabilities.FUNCTION_CALLING,
      ModelCapabilities.STRUCTURED_OUTPUT,
    ],
    contextSize: 131072,
    maxTokens: 16384,
  },
  {
    id: "google/gemini-2.5-flash",
    name: "Gemini 2.5 Flash",
    description:
      "Google's Gemini 2.5 Flash model, optimized for fast responses.",
    provider: Provider.GOOGLE,
    capabilities: [
      ModelCapabilities.CHAT,
      ModelCapabilities.COMPLETION,
      ModelCapabilities.FUNCTION_CALLING,
      ModelCapabilities.STRUCTURED_OUTPUT,
      ModelCapabilities.SEARCH,
    ],
    inputModalities: [
      Modalities.TEXT,
      Modalities.IMAGE,
      Modalities.VIDEO,
      Modalities.AUDIO,
    ],
    outputModalities: [Modalities.TEXT],
    contextSize: 1048576,
    maxTokens: 65536,
  },
  {
    id: "google/gemini-2.5-flash-thinking",
    name: "Gemini 2.5 Flash (Thinking)",
    description:
      "Google's Gemini 2.5 Flash model with enhanced reasoning capabilities.",
    provider: Provider.GOOGLE,
    capabilities: [
      ModelCapabilities.CHAT,
      ModelCapabilities.COMPLETION,
      ModelCapabilities.FUNCTION_CALLING,
      ModelCapabilities.STRUCTURED_OUTPUT,
      ModelCapabilities.SEARCH,
    ],
    inputModalities: [
      Modalities.TEXT,
      Modalities.IMAGE,
      Modalities.VIDEO,
      Modalities.AUDIO,
    ],
    outputModalities: [Modalities.TEXT],
    contextSize: 1048576,
    maxTokens: 65536,
  },
  {
    id: "google/gemini-2.5-pro",
    name: "Gemini 2.5 Pro",
    description:
      "Google's Gemini 2.5 Pro model, optimized for professional tasks.",
    provider: Provider.GOOGLE,
    capabilities: [
      ModelCapabilities.CHAT,
      ModelCapabilities.COMPLETION,
      ModelCapabilities.FUNCTION_CALLING,
      ModelCapabilities.STRUCTURED_OUTPUT,
      ModelCapabilities.SEARCH,
    ],
    inputModalities: [
      Modalities.TEXT,
      Modalities.IMAGE,
      Modalities.VIDEO,
      Modalities.AUDIO,
      Modalities.PDF,
    ],
    outputModalities: [Modalities.TEXT],
    contextSize: 1048576,
    maxTokens: 65536,
  },
  {
    id: "openai/gpt-4.1",
    name: "GPT-4.1",
    description: "OpenAI's GPT-4.1 model, optimized for general tasks.",
    provider: Provider.OPENAI,
    capabilities: [
      ModelCapabilities.CHAT,
      ModelCapabilities.COMPLETION,
      ModelCapabilities.FUNCTION_CALLING,
      ModelCapabilities.STRUCTURED_OUTPUT,
      ModelCapabilities.CODE_EXECUTION,
      ModelCapabilities.TUNING,
      ModelCapabilities.SEARCH,
    ],
    inputModalities: [Modalities.TEXT, Modalities.IMAGE],
    outputModalities: [Modalities.TEXT],
    contextSize: 1048576,
    maxTokens: 32768,
  },
  {
    id: "openai/o4-mini",
    name: "o4-mini",
    description:
      "OpenAI's o4-mini model, optimized for fast, effective reasoning with exceptionally efficient performance in coding and visual tasks.",
    provider: Provider.OPENAI,
    capabilities: [
      ModelCapabilities.CHAT,
      ModelCapabilities.COMPLETION,
      ModelCapabilities.FUNCTION_CALLING,
      ModelCapabilities.STRUCTURED_OUTPUT,
      ModelCapabilities.CODE_EXECUTION,
      ModelCapabilities.TUNING,
      ModelCapabilities.SEARCH,
    ],
    inputModalities: [Modalities.TEXT, Modalities.IMAGE],
    outputModalities: [Modalities.TEXT],
    contextSize: 200000,
    maxTokens: 100000,
  },
  {
    id: "anthropic/claude-sonnet-4-0",
    name: "Claude Sonnet 4",
    description:
      "Anthropic's Claude Sonnet 4 model, optimized for advanced reasoning and tool use.",
    provider: Provider.OPENAI,
    capabilities: [
      ModelCapabilities.CHAT,
      ModelCapabilities.COMPLETION,
      ModelCapabilities.FUNCTION_CALLING,
      ModelCapabilities.STRUCTURED_OUTPUT,
      ModelCapabilities.SEARCH,
    ],
    inputModalities: [Modalities.TEXT, Modalities.IMAGE],
    outputModalities: [Modalities.TEXT],
    contextSize: 200000,
    maxTokens: 100000,
  },
];

```

---

## API Endpoints
Implement full CRUD operations for all endpoints under `/api/v1/`. All require bearer token API key authentication.

| Resource      | Path                    | Operations | Description                 |
| ------------- | ----------------------- | ---------- | --------------------------- |
| Models        | `/api/v1/models`        | CRUD       | Manage model configurations |
| Organizations | `/api/v1/organizations` | CRUD       | Manage organizations        |
| Projects      | `/api/v1/projects`      | CRUD       | Manage projects             |
| Agents        | `/api/v1/agents`        | CRUD       | Manage agent configurations |

- Use Chanfana for OpenAPI support, serving the spec at `/api/v1/openapi.json`.
- Endpoints must validate authentication and user permissions (e.g., org access).

---

## Implementation Guidelines
1. **D1 Auto Endpoints**
   - Simple crud API's like this need very crisp database schemas so that the database takes care of the validation
   - This needs to use [`D1AutoEndpoints` from Chanfana](https://chanfana.pages.dev/endpoints/auto/d1). You MUST review these docs

2. **Authentication**
   - Enforce bearer token API key validation on all endpoints.
   - Integrate with user/organization checks for access control.

3. **Documentation & Grounding**
   - Ground solutions in latest documentation using Context7 and available tools (your knowledge cutoff requires this for up-to-date info).

4. **Testing**
   - Unit tests: Models and API logic.
   - Integration tests: Full API endpoints (including auth flows).

5. **File locations**
   - This assignment should be executed in workers/agent-registry-api

---

## Success Criteria
- [ ] API served from `/api/v1` with bearer auth on all endpoints.
- [ ] OpenAPI spec served from `/api/v1/openapi.json`.
- [ ] All CRUD endpoints defined and functional.
- [ ] Unit tests for models.
- [ ] Unit tests for API logic.
- [ ] Integration tests for API (covering CRUD + auth).

## Avoid
- Cloudflare worker types (use `wrangler types` instead of installing `@cloudflare/workers-types`).

## Resources
- [Cloudflare Developers](https://developers.cloudflare.com/llms.txt)
- [Hono Framework](https://hono.dev/llms.txt)
- [Chanfana Introduction](https://chanfana.pages.dev/introduction)
