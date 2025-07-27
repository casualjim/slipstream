-- Seed organizations
INSERT INTO
  organizations (slug, name, description)
VALUES
  (
    'wagyu',
    'Wagyu',
    'The family organization'
  );

-- Seed projects
INSERT INTO
  projects (slug, name, description, organization)
VALUES
  (
    'wagyu-project',
    'Default',
    'Default project under the Wagyu organization',
    'wagyu'
  );

-- Seed users
INSERT INTO
  users (id, name, email, organizations)
VALUES
  (
    '01983d84-2e1d-747d-8e58-fdeb3f5d241d',
    'Test User',
    'test.user@example.com',
    '["wagyu"]'
  );

-- Seed model providers
INSERT INTO
  model_providers (
    id,
    name,
    provider,
    description,
    contextSize,
    maxTokens,
    temperature,
    topP,
    frequencyPenalty,
    presencePenalty,
    capabilities,
    inputModalities,
    outputModalities,
    dialect
  )
VALUES
  -- DeepSeek Models
  (
    'deepseek-ai/DeepSeek-R1-0528',
    'DeepSeek R1',
    'DeepInfra',
    'DeepSeek R1 model, optimized for reasoning tasks.',
    163840,
    16384,
    0.7,
    0.9,
    0.0,
    0.0,
    '["chat","completion","function_calling","structured_output","thinking"]',
    '["text"]',
    '["text"]',
    'deepseek'
  ),
  (
    'deepseek-ai/DeepSeek-V3-0324',
    'DeepSeek V3',
    'DeepInfra',
    'DeepSeek V3 model, optimized for general tasks.',
    163840,
    16384,
    0.7,
    0.9,
    0.0,
    0.0,
    '["chat","completion","function_calling","structured_output"]',
    '["text"]',
    '["text"]',
    'deepseek'
  ),
  -- Kimi Models
  (
    'moonshotai/Kimi-K2-Instruct',
    'Kimi K2',
    'DeepInfra',
    'Kimi K2 model, optimized for agentic capabilities, including advanced tool use, reasoning, and code synthesis',
    131072,
    16384,
    0.7,
    0.9,
    0.0,
    0.0,
    '["chat","completion","function_calling","structured_output"]',
    '["text"]',
    '["text"]',
    'anthropic'
  ),
  -- Google Gemini Models
  (
    'google/gemini-2.5-flash',
    'Gemini 2.5 Flash',
    'Google',
    'Google''s Gemini 2.5 Flash model, optimized for fast responses.',
    1048576,
    65536,
    0.7,
    0.9,
    0.0,
    0.0,
    '["chat","completion","function_calling","structured_output","search"]',
    '["text","image","video","audio"]',
    '["text"]',
    'openai'
  ),
  (
    'google/gemini-2.5-flash-thinking',
    'Gemini 2.5 Flash (Thinking)',
    'Google',
    'Google''s Gemini 2.5 Flash model with enhanced reasoning capabilities.',
    1048576,
    65536,
    0.7,
    0.9,
    0.0,
    0.0,
    '["chat","completion","function_calling","structured_output","search"]',
    '["text","image","video","audio"]',
    '["text"]',
    'openai'
  ),
  (
    'google/gemini-2.5-pro',
    'Gemini 2.5 Pro',
    'Google',
    'Google''s Gemini 2.5 Pro model, optimized for professional tasks.',
    1048576,
    65536,
    0.7,
    0.9,
    0.0,
    0.0,
    '["chat","completion","function_calling","structured_output","search"]',
    '["text","image","video","audio","pdf"]',
    '["text"]',
    'openai'
  ),
  -- OpenAI Models
  (
    'openai/gpt-4.1',
    'GPT-4.1',
    'OpenAI',
    'OpenAI''s GPT-4.1 model, optimized for general tasks.',
    1048576,
    32768,
    0.7,
    0.9,
    0.0,
    0.0,
    '["chat","completion","function_calling","structured_output","code_execution","tuning","search"]',
    '["text","image"]',
    '["text"]',
    'openai'
  ),
  (
    'openai/o4-mini',
    'o4-mini',
    'OpenAI',
    'OpenAI''s o4-mini model, optimized for fast, effective reasoning with exceptionally efficient performance in coding and visual tasks.',
    200000,
    100000,
    0.7,
    0.9,
    0.0,
    0.0,
    '["chat","completion","function_calling","structured_output","code_execution","tuning","search"]',
    '["text","image"]',
    '["text"]',
    'openai'
  ),
  -- Anthropic Models
  (
    'anthropic/claude-sonnet-4-0',
    'Claude Sonnet 4',
    'OpenAI',
    'Anthropic''s Claude Sonnet 4 model, optimized for advanced reasoning and tool use.',
    200000,
    100000,
    0.7,
    0.9,
    0.0,
    0.0,
    '["chat","completion","function_calling","structured_output","search"]',
    '["text","image"]',
    '["text"]',
    'anthropic'
  );

-- Seed tools
INSERT INTO
  tools (slug, version, name, description, provider, arguments)
VALUES
  (
    'web-search',
    '1.0.0',
    'web_search',
    'Search the web for information',
    'Local',
    '{"type":"object","properties":{"query":{"type":"string","description":"The search query"}},"required":["query"]}'
  ),
  (
    'code-interpreter',
    '1.0.0',
    'code_interpreter',
    'Execute code in a sandboxed environment',
    'Local',
    '{"type":"object","properties":{"code":{"type":"string","description":"The code to execute"},"language":{"type":"string","enum":["python","javascript","typescript"],"description":"The programming language"}},"required":["code","language"]}'
  ),
  (
    'file-operations',
    '1.0.0',
    'file_operations',
    'Read, write, and manage files',
    'Local',
    '{"type":"object","properties":{"operation":{"type":"string","enum":["read","write","delete","list"],"description":"The file operation to perform"},"path":{"type":"string","description":"The file path"},"content":{"type":"string","description":"Content to write (for write operations)"}},"required":["operation","path"]}'
  );
