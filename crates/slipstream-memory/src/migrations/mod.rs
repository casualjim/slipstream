//! Database migrations for the Kumos knowledge graph
//!
//! This module contains all migrations for nodes and edges.
//! Each type gets its own migration to ensure proper ordering.

pub mod concept_migration;
pub mod includes_migration;
pub mod interaction_migration;
pub mod mentions_migration;
pub mod relates_migration;
pub mod theme_migration;
