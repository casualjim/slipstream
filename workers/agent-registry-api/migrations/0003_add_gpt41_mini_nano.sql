-- Add gpt-4.1-mini and gpt-4.1-nano models to model_providers with accurate context size and max tokens

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
  (
    'openai/gpt-4.1-mini',
    'GPT-4.1 Mini',
    'OpenAI',
    'OpenAI''s GPT-4.1 Mini model, optimized for lightweight, fast general tasks.',
    1000000,
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
    'openai/gpt-4.1-nano',
    'GPT-4.1 Nano',
    'OpenAI',
    'OpenAI''s GPT-4.1 Nano model, optimized for ultra-fast, efficient reasoning and coding tasks.',
    1000000,
    32768,
    0.7,
    0.9,
    0.0,
    0.0,
    '["chat","completion","function_calling","structured_output","code_execution","tuning","search"]',
    '["text","image"]',
    '["text"]',
    'openai'
  );
