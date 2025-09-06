# Feature Update Pattern — Prompt Template (Forward-Only Migration, jiff Mapping, Restate Wiring)

<task>
{{argument_1}}
</task>

<purpose>
Safely evolve schema and code with a single-path, forward-only change (no toggles or fallbacks). Update the migration, queries, repository mapping, service orchestration, and the Restate handler. All code blocks are examples to adapt to {{argument_1}}.
</purpose>

<scenario>
Describe the concrete change from {{argument_1}} (e.g., new columns/tables, indices, constraints) and where to apply it.
</scenario>

<steps_migration>
```sql path=null start=null
-- migrations/TS_add_feature.sql   # replace filename and content per {{argument_1}}
-- Example for adding a column and index
ALTER TABLE users
  ADD COLUMN last_login_at timestamptz;

CREATE INDEX IF NOT EXISTS users_last_login_at_idx
  ON users (last_login_at DESC);
```
</steps_migration>

<steps_queries>
```sql path=null start=null
-- queries/users/get.sql (include the new column; cast to text for jiff parsing when needed)
SELECT
  id,
  email,
  name,
  picture,
  to_json(created_at)::text AS created_at,
  to_json(updated_at)::text AS updated_at,
  to_json(last_login_at)::text AS last_login_at
FROM users
WHERE id = $1;
```
```sql path=null start=null
-- queries/users/set_last_login.sql
UPDATE users
SET last_login_at = now(),
    updated_at = now()
WHERE id = $1
RETURNING
  id,
  email,
  name,
  picture,
  to_json(created_at)::text AS created_at,
  to_json(updated_at)::text AS updated_at,
  to_json(last_login_at)::text AS last_login_at;
```
</steps_queries>

<repo_mapping>
```rust path=null start=null
#[derive(sqlx::FromRow, Debug)]
struct UserRow {
  pub id: uuid::Uuid,
  pub email: String,
  pub name: String,
  pub picture: Option<String>,
  pub created_at: String,
  pub updated_at: String,
  pub last_login_at: Option<String>,
}

#[derive(Debug, Clone)]
pub struct User {
  pub id: uuid::Uuid,
  pub email: String,
  pub name: String,
  pub picture: Option<String>,
  pub created_at: jiff::Timestamp,
  pub updated_at: jiff::Timestamp,
  pub last_login_at: Option<jiff::Timestamp>,
}

impl From<UserRow> for User {
  fn from(r: UserRow) -> Self {
    let created = jiff::Timestamp::parse(&r.created_at).expect("valid");
    let updated = jiff::Timestamp::parse(&r.updated_at).expect("valid");
    let last_login = r.last_login_at.as_deref().map(|s| jiff::Timestamp::parse(s).expect("valid"));
    Self { id: r.id, email: r.email, name: r.name, picture: r.picture, created_at: created, updated_at: updated, last_login_at: last_login }
  }
}

pub async fn set_last_login_tx(
  tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  user_id: uuid::Uuid,
) -> crate::Result<User> {
  let row = sqlx::query_file_as!(UserRow, "queries/users/set_last_login.sql", user_id)
    .fetch_one(&mut **tx)
    .await?;
  Ok(row.into())
}
```
</repo_mapping>

<service_update>
```rust path=null start=null
pub async fn after_auth_success(pool: &sqlx::PgPool, user_id: uuid::Uuid) -> crate::Result<()> {
  let mut tx = pool.begin().await?;
  crate::repo::users::set_last_login_tx(&mut tx, user_id).await?;
  tx.commit().await?;
  Ok(())
}
```
</service_update>

<handler_update>
```rust path=null start=null
use restate_sdk::prelude::*;

#[derive(serde::Deserialize, schemars::JsonSchema)]
pub struct LoginArgs { pub user_id: String }

#[derive(Clone)]
pub struct AuthServiceImpl { pub pool: std::sync::Arc<sqlx::PgPool> }

#[restate_sdk::service]
pub trait AuthService {
  async fn mark_login(args: Json<LoginArgs>) -> Result<(), HandlerError>;
}

impl AuthService for AuthServiceImpl {
  async fn mark_login(&self, _ctx: Context<'_>, Json(args): Json<LoginArgs>) -> Result<(), HandlerError> {
    let user_id = uuid::Uuid::parse_str(&args.user_id)
      .map_err(|_| eyre::eyre!("invalid user_id"))?;
    crate::service::auth::after_auth_success(&self.pool, user_id).await?;
    Ok(())
  }
}
```
</handler_update>

<tests>
- Apply migrations: sqlx migrate run
- Seed data (e.g., upsert) and read back using updated queries
- Assert new behavior (e.g., last_login_at set) and remove obsolete code
</tests>

<cleanup>
- Remove redundant paths; keep a single way to perform the behavior
</cleanup>

<acceptance>
- Migration applies forward cleanly (no toggles)
- Updated queries compile-check (DATABASE_URL required)
- Domain uses jiff; mapping updated consistently
- Service composes repo calls in one transaction
- Handler validates and delegates to service
- UUID v7 policy intact
</acceptance>

<do_dont>
Do: update exactly the files listed; keep diffs minimal and focused
Don’t: add fallbacks, feature flags, or alternative flows for the same behavior
</do_dont>

