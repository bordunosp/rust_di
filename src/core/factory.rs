use crate::DIScope;
use crate::core::error_di::DiError;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait DiFactory: Send + Sync + 'static {
    async fn create(scope: Arc<DIScope>) -> Result<Self, DiError>
    where
        Self: Sized;
}
