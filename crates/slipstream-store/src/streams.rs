use crate::Result;
use futures::Stream;
use std::future::Future;
use std::pin::Pin;

pub type DefaultStream<T> = Pin<Box<dyn Stream<Item = T> + Send + 'static>>;

/// Type alias for the most common case - a stream of Results
pub type ResultStream<T> = DefaultStream<Result<T>>;

/// A boxed future that returns a Result<T>
pub type ResultFuture<T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'static>>;

/// Graph index values stream (Kuzu rows)
pub type GraphValuesStream = ResultStream<Vec<kuzu::Value>>;
