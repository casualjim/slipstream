pub type Result<T, E = Error> = eyre::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum Error {}
