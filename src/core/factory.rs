use crate::DIScope;
use crate::core::error_di::AnyError;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait DiFactory<TError>: Send + Sync + 'static
where
    TError: AnyError,
{
    async fn create(scope: Arc<DIScope>) -> Result<Self, TError>
    where
        Self: Sized;
}
