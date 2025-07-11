use std::error::Error;
use thiserror::Error;

pub trait AnyError: Error + From<DiError> + Send + Sync + 'static {}
impl<T> AnyError for T where T: Error + From<DiError> + Send + Sync + 'static {}

#[derive(Debug, Error)]
pub enum DiError {
    #[error("DiError: Service not found with name: {0}")]
    ServiceNotFound(String),

    #[error("DiError: Service already registered with name: {0}")]
    ServiceAlreadyRegistered(String),

    #[error("DiError: A Mutex or RwLock was poisoned")]
    LockPoisoned,

    #[error("DiError: Service factory error: {0}")]
    FactoryError(Box<dyn Error + Send + Sync + 'static>),

    #[error("DiError: Circular dependency detected for with name: {0}")]
    CircularDependency(String),
}
