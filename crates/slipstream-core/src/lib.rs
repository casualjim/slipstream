pub mod definitions;
pub mod messages;
pub mod registry;

type Result<T, E = eyre::Report> = eyre::Result<T, E>;
