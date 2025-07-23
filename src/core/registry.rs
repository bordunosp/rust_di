use crate::DIScope;
use crate::core::contracts::{RegisteredInstances, ServiceInstance};
use crate::core::error_di::DiError;
use arc_swap::ArcSwap;
use dashmap::DashMap;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{OnceCell, RwLock as TokioRwLock};

pub(crate) static REGISTERED_SINGLETON_FACTORIES: RegisteredInstances = OnceCell::const_new();
pub(crate) static REGISTERED_TRANSIENT_FACTORIES: RegisteredInstances = OnceCell::const_new();
pub(crate) static REGISTERED_SCOPE_FACTORIES: RegisteredInstances = OnceCell::const_new();

pub(crate) static SINGLETON_CACHE: once_cell::sync::OnceCell<
    DashMap<(String, String), ServiceInstance>,
> = once_cell::sync::OnceCell::new();

#[allow(dead_code)]
pub(crate) async fn register_factory<T, F, Fut>(
    name: &str,
    factory: F,
    registry: &'static RegisteredInstances,
) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    let type_key = std::any::type_name::<T>().to_string();
    let name_string = name.to_string();
    let factories_arcswap = registry
        .get_or_init(|| async { ArcSwap::from_pointee(DashMap::new()) })
        .await;

    let key = (type_key, name_string.clone());
    let factories = factories_arcswap.load();

    use dashmap::mapref::entry::Entry;
    match factories.entry(key.clone()) {
        Entry::Occupied(_) => return Err(DiError::ServiceAlreadyRegistered(name_string)),
        Entry::Vacant(entry) => {
            let arc_factory = Arc::new(factory);
            let wrapped_factory = Arc::new(move |scope: Arc<DIScope>| {
                let factory_cloned = arc_factory.clone();
                Box::pin(async move {
                    let service = factory_cloned(scope).await?;
                    Ok(Arc::new(TokioRwLock::new(service)) as ServiceInstance)
                })
                    as Pin<Box<dyn Future<Output = Result<ServiceInstance, DiError>> + Send>>
            });

            entry.insert(wrapped_factory);
        }
    }

    Ok(())
}

#[allow(dead_code)]
pub async fn register_transient<T, F, Fut>(factory: F) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    register_factory("", factory, &REGISTERED_TRANSIENT_FACTORIES).await
}

#[allow(dead_code)]
pub async fn register_transient_name<T, F, Fut>(name: &str, factory: F) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    register_factory(name, factory, &REGISTERED_TRANSIENT_FACTORIES).await
}

#[allow(dead_code)]
pub async fn register_scope<T, F, Fut>(factory: F) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    register_factory("", factory, &REGISTERED_SCOPE_FACTORIES).await
}

#[allow(dead_code)]
pub async fn register_scope_name<T, F, Fut>(name: &str, factory: F) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    register_factory(name, factory, &REGISTERED_SCOPE_FACTORIES).await
}

#[allow(dead_code)]
pub async fn register_singleton<T, F, Fut>(factory: F) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    register_factory("", factory, &REGISTERED_SINGLETON_FACTORIES).await
}

#[allow(dead_code)]
pub async fn register_singleton_name<T, F, Fut>(name: &str, factory: F) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    register_factory(name, factory, &REGISTERED_SINGLETON_FACTORIES).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{DIScope, initialize};
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct UniqueSingletonService;

    #[tokio::test]
    async fn test_unique_singleton_register_and_resolve() {
        initialize().await;
        register_singleton::<UniqueSingletonService, _, _>(|_| async {
            Ok(UniqueSingletonService)
        })
        .await
        .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let resolved = scope.get::<UniqueSingletonService>().await.unwrap();
            let guard = resolved.read().await;
            let ptr = &*guard as *const UniqueSingletonService;
            assert!(!ptr.is_null());
        })
        .await;
    }

    #[derive(Default)]
    struct DuplicateCheckService;

    #[tokio::test]
    async fn test_unique_singleton_duplicate_should_fail() {
        initialize().await;
        let _ = register_singleton_name::<DuplicateCheckService, _, _>("duplicate", |_| async {
            Ok(DuplicateCheckService)
        })
        .await;

        let result =
            register_singleton_name::<DuplicateCheckService, _, _>("duplicate", |_| async {
                Ok(DuplicateCheckService)
            })
            .await;

        assert!(matches!(result, Err(DiError::ServiceAlreadyRegistered(_))));
    }

    #[tokio::test]
    async fn test_unique_transient_returns_new_instance() {
        initialize().await;
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        #[derive(Default)]
        struct TransientCounterService(usize);

        register_transient::<TransientCounterService, _, _>(|_| async {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(TransientCounterService(id))
        })
        .await
        .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let a = scope
                .clone()
                .get::<TransientCounterService>()
                .await
                .unwrap();
            let b = scope
                .clone()
                .get::<TransientCounterService>()
                .await
                .unwrap();

            let a_id = a.read().await.0;
            let b_id = b.read().await.0;

            assert_ne!(a_id, b_id);
        })
        .await;
    }

    #[tokio::test]
    async fn test_unique_scoped_returns_same_instance_within_scope() {
        initialize().await;

        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        #[derive(Default)]
        struct ScopedCounterService(usize);

        register_scope::<ScopedCounterService, _, _>(|_| async {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(ScopedCounterService(id))
        })
        .await
        .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let a = scope.clone().get::<ScopedCounterService>().await.unwrap();
            let b = scope.clone().get::<ScopedCounterService>().await.unwrap();

            let a_id = a.read().await.0;
            let b_id = b.read().await.0;

            assert_eq!(a_id, b_id);
        })
        .await;
    }

    #[tokio::test]
    async fn test_unique_named_registration_and_resolution() {
        initialize().await;

        #[derive(Default)]
        struct NamedAlphaBetaService(&'static str);

        register_singleton_name::<NamedAlphaBetaService, _, _>("alpha", |_| async {
            Ok(NamedAlphaBetaService("alpha"))
        })
        .await
        .unwrap();

        register_singleton_name::<NamedAlphaBetaService, _, _>("beta", |_| async {
            Ok(NamedAlphaBetaService("beta"))
        })
        .await
        .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let alpha = scope
                .clone()
                .get_by_name::<NamedAlphaBetaService>("alpha")
                .await
                .unwrap();
            let beta = scope
                .clone()
                .get_by_name::<NamedAlphaBetaService>("beta")
                .await
                .unwrap();

            assert_eq!(alpha.read().await.0, "alpha");
            assert_eq!(beta.read().await.0, "beta");
        })
        .await;
    }
}
