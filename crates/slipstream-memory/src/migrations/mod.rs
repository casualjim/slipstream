//! Database migrations for the Kumos knowledge graph
//!
//! This module contains all migrations for nodes and edges.
//! Each type gets its own migration to ensure proper ordering.

mod concept_migration;
mod includes_migration;
mod interaction_migration;
mod mentions_migration;
mod relates_migration;
mod theme_migration;

pub use concept_migration::RunConceptMigration;
pub use includes_migration::RunIncludesMigration;
pub use interaction_migration::RunInteractionMigration;
pub use mentions_migration::RunMentionsMigration;
pub use relates_migration::RunRelatesMigration;
pub use theme_migration::RunThemeMigration;
