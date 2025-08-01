use crate::{AgentDefinition, AgentRef};
use crate::{Pagination, Registry, Result};
use async_trait::async_trait;
use semver::Version;

#[derive(Debug, Clone)]
pub struct MemoryAgentRegistry {
  // Keyed by "slug" or "slug/version"; for versionless GET/HAS we consult the "latest" pointer
  store: dashmap::DashMap<String, AgentDefinition>,
}

impl Default for MemoryAgentRegistry {
  fn default() -> Self {
    Self::new()
  }
}

impl MemoryAgentRegistry {
  pub fn new() -> Self {
    Self {
      store: dashmap::DashMap::new(),
    }
  }
}

#[async_trait]
impl Registry for MemoryAgentRegistry {
  type Subject = AgentDefinition;
  type Key = AgentRef;

  /// Create or update requires version to be present.
  async fn put(&self, name: Self::Key, subject: Self::Subject) -> Result<()> {
    if name.version.is_none() {
      return Err(crate::Error::Registry {
        reason: "Version is required for put".to_string(),
        status_code: None,
      });
    }
    // Persist the concrete version record under "slug/version"
    let version_key = format!("{}/{}", name.slug, name.version.as_ref().unwrap());
    self.store.insert(version_key, subject.clone());
    // Update "latest" pointer under "slug" to the provided subject
    self.store.insert(name.slug.clone(), subject);
    Ok(())
  }

  /// Delete requires version; we delete that specific version and, if it was the latest, we adjust latest pointer.
  async fn del(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    if name.version.is_none() {
      return Err(crate::Error::Registry {
        reason: "Version is required for delete".to_string(),
        status_code: None,
      });
    }
    let version_key = format!("{}/{}", name.slug, name.version.as_ref().unwrap());
    // Capture the versioned record to return
    let removed = if let Some((_, removed)) = self.store.remove(&version_key) {
      Some(removed)
    } else {
      None
    };

    // If we removed something and the "latest" points to the same version, recompute latest
    if let Some(ref removed_subject) = removed {
      if let Some(latest) = self.store.get(&name.slug) {
        // if latest version matches removed version, recompute latest from remaining versions
        if latest.version == removed_subject.version {
          drop(latest);
          // find remaining versions for this slug
          let prefix = format!("{}/", name.slug);
          let mut candidates: Vec<AgentDefinition> = self
            .store
            .iter()
            .filter_map(|kv| {
              let k = kv.key();
              if k.starts_with(&prefix) {
                Some(kv.value().clone())
              } else {
                None
              }
            })
            .collect();
          // Select latest by semantic version ordering; fall back to string compare on parse errors
          candidates.sort_by(|a, b| {
            match (
              semver::Version::parse(&a.version),
              semver::Version::parse(&b.version),
            ) {
              (Ok(va), Ok(vb)) => va.cmp(&vb),
              _ => a.version.cmp(&b.version),
            }
          });
          let new_latest = candidates.pop();
          if let Some(def) = new_latest {
            self.store.insert(name.slug.clone(), def);
          } else {
            // no remaining versions; remove latest pointer
            let _ = self.store.remove(&name.slug);
          }
        }
      }
    }

    Ok(removed)
  }

  /// Get supports versionless lookups: when version is None, return the latest (value under "slug").
  async fn get(&self, name: Self::Key) -> Result<Option<Self::Subject>> {
    let key = if let Some(v) = &name.version {
      format!("{}/{}", name.slug, v)
    } else {
      name.slug.clone()
    };
    if let Some(subject) = self.store.get(&key) {
      Ok(Some(subject.clone()))
    } else {
      Ok(None)
    }
  }

  /// Has supports versionless lookups: when version is None, check "slug" key for latest.
  async fn has(&self, name: Self::Key) -> Result<bool> {
    let key = if let Some(v) = &name.version {
      format!("{}/{}", name.slug, v)
    } else {
      name.slug.clone()
    };
    Ok(self.store.contains_key(&key))
  }

  async fn keys(&self, pagination: Pagination) -> Result<Vec<String>> {
    // Return all keys: latest pointers (plain "slug") and all versioned "slug/version" entries
    let mut keys: Vec<String> = self.store.iter().map(|kv| kv.key().to_string()).collect();

    // Sort for consistent pagination
    keys.sort();

    // 1-based page, default to 1 if not provided
    let page = pagination.page.unwrap_or(1).max(1);
    let per_page = pagination
      .per_page
      .unwrap_or_else(|| keys.len().max(1))
      .max(1);

    let start_index = (page - 1) * per_page;
    let end_index = std::cmp::min(start_index + per_page, keys.len());

    let paged_keys = if start_index < keys.len() {
      keys[start_index..end_index].to_vec()
    } else {
      Vec::new()
    };

    Ok(paged_keys)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::AgentDefinition;
  use std::sync::Arc;

  fn assert_agent_eq(a: &AgentDefinition, b: &AgentDefinition) {
    assert_eq!(a.name, b.name);
    assert_eq!(a.description, b.description);
    assert_eq!(a.version, b.version);
    assert_eq!(a.slug, b.slug);
    assert_eq!(a.instructions, b.instructions);

    // Compare ModelDefinition fields
    assert_eq!(a.model, b.model);

    // Compare available_tools (now Vec<String>)
    assert_eq!(a.available_tools, b.available_tools);
  }

  fn create_test_agent(name: &str) -> AgentDefinition {
    AgentDefinition {
      name: name.to_string(),
      model: "gpt-4.1-nano".to_string(),
      description: Some(format!("Test agent {name}")),
      version: "1.0.0".to_string(),
      slug: format!("agent-{name}"),
      available_tools: vec!["local/tool1/1.0.0".to_string()],
      instructions: "You are a helpful assistant".to_string(),
      organization: "test-org".to_string(),
      project: "test-project".to_string(),
    }
  }

  fn create_registry() -> MemoryAgentRegistry {
    MemoryAgentRegistry::new()
  }

  /// Test template function for agent registry (versioned paths)
  pub async fn test_agent_registry_basic_operations(
    registry: Arc<dyn Registry<Subject = AgentDefinition, Key = AgentRef>>,
  ) {
    let agent1 = create_test_agent("agent1");
    let agent2 = create_test_agent("agent2");

    let key1 = AgentRef::from(&agent1);
    let key2 = AgentRef::from(&agent2);
    // For a nonexistent key, create a dummy AgentDefinition
    let agent_nonexistent = create_test_agent("nonexistent");
    let key_nonexistent = AgentRef::from(&agent_nonexistent);

    // Test put and get
    registry.put(key1.clone(), agent1.clone()).await.unwrap();
    let retrieved = registry.get(key1.clone()).await.unwrap();
    assert!(retrieved.is_some());
    assert_agent_eq(&retrieved.unwrap(), &agent1);

    // Test has
    assert!(registry.has(key1.clone()).await.unwrap());
    assert!(!registry.has(key_nonexistent.clone()).await.unwrap());

    // Test put another agent
    registry.put(key2.clone(), agent2.clone()).await.unwrap();

    // Test keys (no pagination) should include both latest pointers and versioned keys
    let keys = registry
      .keys(Pagination {
        page: None,
        per_page: None,
      })
      .await
      .unwrap();
    let mut expected_keys = vec![
      agent1.slug.clone(),
      agent2.slug.clone(),
      format!("{}/{}", agent1.slug, agent1.version),
      format!("{}/{}", agent2.slug, agent2.version),
    ];
    expected_keys.sort();
    assert_eq!(keys, expected_keys);

    // Test delete
    let deleted = registry.del(key1.clone()).await.unwrap();
    assert!(deleted.is_some());
    assert_agent_eq(&deleted.unwrap(), &agent1);
    assert!(!registry.has(key1.clone()).await.unwrap());

    // Verify agent1 is gone
    let retrieved = registry.get(key1.clone()).await.unwrap();
    assert!(retrieved.is_none());

    // Delete non-existent
    let deleted = registry.del(key_nonexistent.clone()).await.unwrap();
    assert!(deleted.is_none());
  }

  #[tokio::test]
  async fn test_memory_agent_registry_basic_operations() {
    let registry = Arc::new(create_registry());
    test_agent_registry_basic_operations(registry).await;
  }

  #[tokio::test]
  async fn test_memory_agent_registry_version_requirements() {
    let registry = create_registry();
    let agent = create_test_agent("vr-agent");
    // versionless key (no version set)
    let versionless_key = AgentRef::builder().slug(agent.slug.clone()).build();

    // PUT without version should error
    let res = registry.put(versionless_key.clone(), agent.clone()).await;
    assert!(res.is_err(), "PUT without version should error");

    // DEL without version should error
    let res = registry.del(versionless_key.clone()).await;
    assert!(res.is_err(), "DEL without version should error");
  }

  #[tokio::test]
  async fn test_memory_agent_registry_versionless_get_has() {
    let registry = create_registry();
    // two versions for same slug, latest should end up as higher semver
    let mut v1 = create_test_agent("ver-agent");
    v1.version = "1.0.0".to_string();
    v1.slug = "ver-agent".to_string();

    let mut v2 = v1.clone();
    v2.version = "1.1.0".to_string();

    // put v1 then v2
    registry.put(AgentRef::from(&v1), v1.clone()).await.unwrap();
    registry.put(AgentRef::from(&v2), v2.clone()).await.unwrap();

    // versionless GET should fetch latest (1.1.0)
    let latest = registry
      .get(AgentRef::builder().slug("ver-agent").build())
      .await
      .unwrap();
    assert_eq!(latest.unwrap().version, "1.1.0");

    // versionless HAS should be true
    let has_latest = registry
      .has(AgentRef::builder().slug("ver-agent").build())
      .await
      .unwrap();
    assert!(has_latest);

    // delete specific version (1.1.0) and ensure latest falls back to 1.0.0
    let _ = registry
      .del(
        AgentRef::builder()
          .slug("ver-agent")
          .version("1.1.0")
          .build(),
      )
      .await
      .unwrap();
    let latest_after = registry
      .get(AgentRef::builder().slug("ver-agent").build())
      .await
      .unwrap();
    assert_eq!(latest_after.unwrap().version, "1.0.0");
  }

  #[tokio::test]
  async fn test_memory_agent_registry_versionless_get_has_latest_pointer() {
    let registry = create_registry();
    // two versions for same slug, latest should end up as higher semver
    let mut v1 = create_test_agent("ver-agent");
    v1.version = "1.0.0".to_string();
    v1.slug = "ver-agent".to_string();

    let mut v2 = v1.clone();
    v2.version = "1.1.0".to_string();

    // put v1 then v2
    registry.put(AgentRef::from(&v1), v1.clone()).await.unwrap();
    registry.put(AgentRef::from(&v2), v2.clone()).await.unwrap();

    // versionless get should fetch latest (1.1.0)
    let latest = registry
      .get(AgentRef::builder().slug("ver-agent").build())
      .await
      .unwrap();
    assert_eq!(latest.unwrap().version, "1.1.0");

    // versionless has should be true
    let has_latest = registry
      .has(AgentRef::builder().slug("ver-agent").build())
      .await
      .unwrap();
    assert!(has_latest);

    // delete specific version (1.1.0) and ensure latest falls back to 1.0.0
    let _ = registry
      .del(
        AgentRef::builder()
          .slug("ver-agent")
          .version("1.1.0")
          .build(),
      )
      .await
      .unwrap();
    let latest_after = registry
      .get(AgentRef::builder().slug("ver-agent").build())
      .await
      .unwrap();
    assert_eq!(latest_after.unwrap().version, "1.0.0");
  }

  #[tokio::test]
  async fn test_memory_agent_registry_update_existing() {
    let registry = create_registry();
    let agent1_v1 = create_test_agent("agent1");
    let mut agent1_v2 = create_test_agent("agent1");
    agent1_v2.model = "gpt-4.1-mini".to_string();

    let key1 = AgentRef::from(&agent1_v1);

    // Put initial version
    registry.put(key1.clone(), agent1_v1).await.unwrap();

    // Update with new version
    registry.put(key1.clone(), agent1_v2.clone()).await.unwrap();

    // Verify updated version is retrieved
    let retrieved = registry.get(key1.clone()).await.unwrap().unwrap();
    assert_eq!(retrieved.model, "gpt-4.1-mini");
  }
}
