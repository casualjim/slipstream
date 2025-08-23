import { rm } from "node:fs/promises";
import { embed } from "ai";
import { Connection, Database, QueryResult } from "kuzu";
import { ollama } from "ollama-ai-provider-v2";

// Config
const DB_PATH = "./db";
const OLLAMA_MODEL = "snowflake-arctic-embed:xs"; // user-specified
const EMBED_DIM = 384; // model dimension for snowflake-arctic-embed:xs

// Utilities
function logSection(title: string) {
  console.log(`\n=== ${title} ===`);
}

function toFixed(n: number, d = 4) {
  return Number.isFinite(n) ? n.toFixed(d) : String(n);
}

async function embedText(text: string): Promise<number[]> {
  const { embedding } = await embed({
    model: ollama.embedding(OLLAMA_MODEL),
    value: text,
  });
  // Ensure plain array
  return Array.isArray(embedding) ? embedding : Array.from(embedding);
}

// Explicitly install & load the needed Kuzu extensions
async function loadExtensions(conn: Connection) {
  logSection("Load extensions");
  // FTS for full-text search
  await conn.query("INSTALL FTS;");
  await conn.query("LOAD EXTENSION FTS;");
  // VECTOR for HNSW vector index/search
  await conn.query("INSTALL VECTOR;");
  await conn.query("LOAD EXTENSION VECTOR;");
}

// Schema DDL
async function setupSchema(conn: Connection) {
  logSection("DDL: recreate schema");
  await conn.query(`CREATE NODE TABLE Author(id STRING, name STRING, bio STRING, PRIMARY KEY(id));`);
  await conn.query(
    `CREATE NODE TABLE Document(id STRING, title STRING, body STRING, publishedAt TIMESTAMP, embedding DOUBLE[${EMBED_DIM}], PRIMARY KEY(id));`,
  );
  await conn.query(`CREATE REL TABLE AUTHORED(FROM Author TO Document);`);
  await conn.query(`CREATE REL TABLE MENTIONS(FROM Document TO Document);`);
}

// Seed data and relationships
async function seedData(conn: Connection) {
  logSection("Seed data");
  const authors = [
    { id: "a1", name: "Ada", bio: "Systems and graphs" },
    { id: "a2", name: "Bob", bio: "Search and indexing" },
    { id: "a3", name: "Cyd", bio: "Embeddings and vectors" },
  ];

  const docs = [
    { id: "d1", title: "Graph Basics", body: "Introduction to graph databases and relationships." },
    { id: "d2", title: "Vector Search", body: "Approximate nearest neighbor and HNSW indexes." },
    { id: "d3", title: "Full-text 101", body: "Tokenization, stemming, stopwords and ranking." },
    { id: "d4", title: "Hybrid Ranking", body: "Blend vector similarity with full-text scores." },
    { id: "d5", title: "Kuzu Pointers", body: "Kuzu supports FTS and vector index for graphs." },
  ];

  for (const a of authors) {
    await conn.query(`CREATE (:Author {id: '${a.id}', name: '${a.name}', bio: '${a.bio}'});`);
  }
  for (const d of docs) {
    const emb = await embedText(`${d.title} ${d.body}`);
    const arr = `[${emb.map((x) => Number(x).toFixed(6)).join(", ")}]`;
    await conn.query(
      `CREATE (:Document {id: '${d.id}', title: '${d.title}', body: '${d.body}', publishedAt: CURRENT_TIMESTAMP(), embedding: ${arr}});`,
    );
  }

  // Relationships
  await conn.query(`MATCH (a:Author {id: 'a1'}), (d:Document {id: 'd1'}) CREATE (a)-[:AUTHORED]->(d);`);
  await conn.query(`MATCH (a:Author {id: 'a2'}), (d:Document {id: 'd2'}) CREATE (a)-[:AUTHORED]->(d);`);
  await conn.query(`MATCH (a:Author {id: 'a3'}), (d:Document {id: 'd3'}) CREATE (a)-[:AUTHORED]->(d);`);
  await conn.query(`MATCH (a:Author {id: 'a1'}), (d:Document {id: 'd4'}) CREATE (a)-[:AUTHORED]->(d);`);

  // Mentions graph
  await conn.query(`MATCH (d1:Document {id: 'd4'}), (d2:Document {id: 'd2'}) CREATE (d1)-[:MENTIONS]->(d2);`);
  await conn.query(`MATCH (d1:Document {id: 'd2'}), (d2:Document {id: 'd3'}) CREATE (d1)-[:MENTIONS]->(d2);`);
}

async function renderResults(res: QueryResult | QueryResult[]): Promise<string> {
  if (Array.isArray(res)) {
    return Promise.all(res.map(renderResults)).then((results) => results.join("\n"));
  }
  return res.getAll().then((rows) => {
    if (rows.length === 0) return "No results";
    return rows.map((row) => JSON.stringify(row)).join("\n");
  });
}

// CRUD for nodes and relationships
async function demoCRUD(conn: Connection) {
  logSection("CRUD demo");

  // Create a doc
  await conn.query(
    `CREATE (:Document {id: 'dx', title: 'Temp Doc', body: 'temp body', publishedAt: CURRENT_TIMESTAMP(), embedding: [${new Array(EMBED_DIM).fill(0).join(", ")}]});`,
  );

  // Read
  let res = await conn.query(`MATCH (d:Document) WHERE d.id IN ['dx','d1'] RETURN d.id, d.title ORDER BY d.id;`);
  console.log(await renderResults(res));

  // Update
  const updatedBody = "updated temp body with graph mention";
  const emb = await embedText("Temp Doc " + updatedBody);
  const arr = `[${emb.map((x) => Number(x).toFixed(6)).join(", ")}]`;
  await conn.query(`MATCH (d:Document {id: 'dx'}) SET d.body = '${updatedBody}', d.embedding = ${arr} RETURN d.id;`);

  // Delete
  await conn.query(`MATCH (d:Document {id: 'dx'}) DELETE d;`);
}

// Graph queries
async function demoGraph(conn: Connection) {
  logSection("Graph queries");

  let res = await conn.query(
    `MATCH (a:Author)-[:AUTHORED]->(d:Document) RETURN a.name AS author, d.title AS title ORDER BY author, title;`,
  );
  renderResults(res).then(console.log);

  res = await conn.query(
    `MATCH p=(d1:Document {id: 'd4'})-[:MENTIONS*1..2]->(d2:Document) RETURN d1.id, d2.id, LENGTH(p) AS L ORDER BY d2.id;`,
  );
  renderResults(res).then(console.log);
}

// Client-side embeddings (no LLM extension)
async function demoClientEmbeddings() {
  logSection("Client-side embeddings (Ollama via Vercel AI SDK)");
  const prompt = "hybrid search over graphs";
  const emb = await embedText(prompt);
  console.log({ embedding_len: emb.length, first3: emb.slice(0, 3).map((x) => Number(x.toFixed(6))) });
}

// Vector index and query
async function demoVectorIndexAndSearch(conn: Connection) {
  logSection("Vector index + vector search");
  // Create HNSW index on Document.embedding
  // Kuzu VECTOR extension exposes CREATE_VECTOR_INDEX + QUERY_VECTOR_INDEX
  await conn.query(
    `CALL CREATE_VECTOR_INDEX('Document', 'doc_vec_idx', 'embedding', metric := 'cosine', ml := 8, mu := 12, efc := 80);`,
  );

  // Build a query vector from text using the same seeded mapping so it matches our stored vectors
  const qText = "vector nearest neighbor index";
  const qEmb = await embedText(qText);
  const qArr = `[${qEmb.map((x) => Number(x).toFixed(6)).join(", ")}]`;

  // Query index for top-5 nearest
  const res = await conn.query(
    `CALL QUERY_VECTOR_INDEX('Document', 'doc_vec_idx', ${qArr}, 5) RETURN node.id AS id, node.title AS title, distance ORDER BY distance;`,
  );
  const rows = await res.getAll();
  for (const r of rows) {
    console.log(`${r.id} | ${r.title} | dist=${toFixed(r.distance)}`);
  }
}

// Full-text index (composite) and search
async function demoFTS(conn: Connection) {
  logSection("Full-text index + search (composite)");
  await conn.query(`CALL CREATE_FTS_INDEX('Document', 'doc_fts', ['title','body'], stemmer := 'english');`);

  let res = await conn.query(
    `CALL QUERY_FTS_INDEX('Document', 'doc_fts', 'graph') RETURN node.id AS id, node.title AS title, score ORDER BY score DESC;`,
  );
  const rows = await res.getAll();
  for (const r of rows) {
    console.log(`${r.id} | ${r.title} | ftsScore=${toFixed(r.score)}`);
  }
}

// Hybrid search: simple re-ranking of top candidates
async function demoHybrid(conn: Connection) {
  logSection("Hybrid search: simple re-ranking");
  const queryText = "graph vector hybrid";

  // Vector side
  const vEmb = await embedText(queryText);
  const vArr = `[${vEmb.map((x) => Number(x).toFixed(6)).join(", ")}]`;
  const vres = await conn.query(
    `CALL QUERY_VECTOR_INDEX('Document', 'doc_vec_idx', ${vArr}, 10) RETURN node.id AS id, 1.0 - distance AS vScore;`,
  );
  const vrows = await vres.getAll();

  // FTS side
  const fres = await conn.query(
    `CALL QUERY_FTS_INDEX('Document', 'doc_fts', '${queryText}') RETURN node.id AS id, score AS tScore;`,
  );
  const frows = await fres.getAll();

  // Join and combine scores
  const tMap = new Map<string, number>();
  for (const r of frows) tMap.set(r.id, Number(r.tScore));
  const wV = 0.6,
    wT = 0.4;
  const merged: { id: string; score: number; vScore: number; tScore: number }[] = [];
  for (const r of vrows) {
    const tScore = tMap.get(r.id) ?? 0;
    const vScore = Number(r.vScore);
    const score = wV * vScore + wT * tScore;
    merged.push({ id: r.id, score, vScore, tScore });
  }
  merged.sort((a, b) => b.score - a.score);
  for (const m of merged.slice(0, 5)) {
    const title = await conn
      .query(`MATCH (d:Document {id: '${m.id}'}) RETURN d.title AS title;`)
      .then(async (r) => (await r.getAll())[0].title);
    console.log(`${m.id} | ${title} | v=${toFixed(m.vScore)} t=${toFixed(m.tScore)} score=${toFixed(m.score)}`);
  }
}

async function main() {
  // Clean DB dir
  await rm(DB_PATH, { recursive: true, force: true });

  // Open DB
  const db = new Database(DB_PATH);
  const conn = new Connection(db);

  await loadExtensions(conn);
  await setupSchema(conn);
  await seedData(conn);

  await demoCRUD(conn);
  await demoGraph(conn);
  await demoClientEmbeddings();
  await demoVectorIndexAndSearch(conn);
  await demoFTS(conn);
  await demoHybrid(conn);

  console.log("\nDone.");
}

if (import.meta.main) {
  main().catch((err) => {
    console.error(err);
    process.exitCode = 1;
  });
}
