# Repository Pattern — Prompt Template (Externalized SQL, jiff Domain Time, SQLx Online Verification)

<task>
{{argument_1}}
</task>

<purpose>
Scaffold a cohesive repository module (DB I/O only) with all SQL externalized and compile-checked via SQLx. All code blocks are examples to adapt precisely to {{argument_1}}.
</purpose>

<constraints>
- All SQL lives in queries/<domain>/*.sql
- Use query_file!/query_file_as! only; no inline SQL strings
- UUID v7; DB DEFAULT uuid_generate_v7()
- Domain time uses jiff (workspace dependency); repos convert to/from DB encodings
- Provide both pool-based and &mut Transaction<'_, Postgres> variants when composition is needed
- Return crate::Result<T>; never unwrap/expect
</constraints>

<output_files>
Create exactly these files for the users repository (adjust names/paths to {{argument_1}}):
- src/repo/users.rs
- queries/users/get.sql
- queries/users/upsert.sql
</output_files>

<steps>
1) Create SQL files under queries/users/ using the exact shapes below
2) Implement src/repo/users.rs with a jiff-based domain struct, a FromRow row struct, and mapping
3) Add both pool-based and transactional variants of repository functions
4) Compose in a service function within a transaction when multiple repo calls are needed
5) Write an integration test that applies migrations, upserts a user, and reads it back
</steps>

<sql_examples>
```sql path=null start=null
-- queries/users/get.sql
SELECT
  id,
  email,
  name,
  picture,
  to_json(created_at)::text AS created_at,
  to_json(updated_at)::text AS updated_at
FROM users
WHERE id = $1;
```
```sql path=null start=null
-- queries/users/upsert.sql
INSERT INTO users(email, name, picture)
  VALUES ($1, $2, $3)
ON CONFLICT (email)
  DO UPDATE SET
    name = EXCLUDED.name,
    picture = EXCLUDED.picture,
    updated_at = now()
RETURNING
  id,
  email,
  name,
  picture,
  to_json(created_at)::text AS created_at,
  to_json(updated_at)::text AS updated_at;
```
</sql_examples>

<repo_module>
```rust path=null start=null
use uuid::Uuid;
use crate::Result;

#[derive(Debug, Clone)]
pub struct User {
  pub id: Uuid,
  pub email: String,
  pub name: String,
  pub picture: Option<String>,
  pub created_at: jiff::Timestamp,
  pub updated_at: jiff::Timestamp,
}

#[derive(sqlx::FromRow, Debug)]
struct UserRow {
  pub id: Uuid,
  pub email: String,
  pub name: String,
  pub picture: Option<String>,
  pub created_at: String,
  pub updated_at: String,
}

impl From<UserRow> for User {
  fn from(r: UserRow) -> Self {
    let created = jiff::Timestamp::parse(&r.created_at).expect("valid timestamp");
    let updated = jiff::Timestamp::parse(&r.updated_at).expect("valid timestamp");
    Self { id: r.id, email: r.email, name: r.name, picture: r.picture, created_at: created, updated_at: updated }
  }
}

pub async fn get_by_id(pool: &sqlx::PgPool, id: Uuid) -> Result<Option<User>> {
  let row = sqlx::query_file_as!(UserRow, "queries/users/get.sql", id)
    .fetch_optional(pool)
    .await?;
  Ok(row.map(Into::into))
}

pub struct UpsertUserInput { pub email: String, pub name: String, pub picture: Option<String> }

pub async fn upsert(pool: &sqlx::PgPool, input: UpsertUserInput) -> Result<User> {
  let row = sqlx::query_file_as!(
    UserRow, "queries/users/upsert.sql", input.email, input.name, input.picture
  ).fetch_one(pool).await?;
  Ok(row.into())
}

pub async fn upsert_tx(
  tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  input: UpsertUserInput,
) -> Result<User> {
  let row = sqlx::query_file_as!(
    UserRow, "queries/users/upsert.sql", input.email, input.name, input.picture
  ).fetch_one(&mut **tx).await?;
  Ok(row.into())
}
```
</repo_module>

<service_composition>
- Handler (Restate): validate inputs (serde + schemars), then call service
- Service: orchestrate repos, open a transaction when composing calls, return a DTO

```rust path=null start=null
pub async fn create_or_update_user(
  pool: &sqlx::PgPool,
  input: crate::repo::users::UpsertUserInput,
) -> crate::Result<crate::repo::users::User> {
  let mut tx = pool.begin().await?;
  let user = crate::repo::users::upsert_tx(&mut tx, input).await?;
  tx.commit().await?;
  Ok(user)
}
```
</service_composition>

<acceptance>
Stop when the repository compiles against a live DATABASE_URL and tests pass.
- All SQL is in queries/, loaded with query_file*/
- UUID v7-only policy is honored (DB defaults to uuid_generate_v7())
- Domain uses jiff; repos map DB → jiff
- Provide transactional variants where composition is needed
- No unwrap/expect; return crate::Result<T>
</acceptance>

<do_dont>
Do: keep functions small and intention-revealing; keep diffs minimal.
Don’t: inline SQL; don’t add feature flags or alternate paths.
</do_dont>

