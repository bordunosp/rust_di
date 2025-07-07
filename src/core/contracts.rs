use crate::DIScope;
use crate::core::error_di::DiError;
use arc_swap::ArcSwap;
use dashmap::DashMap;
use std::any::{Any, TypeId};
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{OnceCell, RwLock as TokioRwLock};

pub trait AnyService: Any + Send + Sync + 'static {}
impl<T: Any + Send + Sync + 'static> AnyService for T {}

pub(crate) type ServiceInstance = Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>;
pub(crate) type ScopedMap = DashMap<ServiceKey, ServiceInstance>;
pub(crate) type FactoryMap = DashMap<ServiceKey, ServiceFactory>;
pub(crate) type ServiceKey = (TypeId, String);
pub(crate) type RegisteredInstances = OnceCell<ArcSwap<FactoryMap>>;
pub(crate) type ServiceFactory = Arc<
    dyn Fn(Arc<DIScope>) -> Pin<Box<dyn Future<Output = Result<ServiceInstance, DiError>> + Send>>
        + Send
        + Sync
        + 'static,
>;
