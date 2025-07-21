use crate::Result;
use futures::Stream;
use std::pin::Pin;

pub type DefaultStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

/// Type alias for the most common case - a stream of Results
pub type ResultStream<T> = DefaultStream<Result<T>>;
