use crate::DIScope;
use crate::core::contracts::{RegisteredInstances, ServiceInstance};
use crate::core::error_di::DiError;
use arc_swap::ArcSwap;
use dashmap::DashMap;
use std::any::TypeId;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::{OnceCell, RwLock as TokioRwLock};

pub(crate) static REGISTERED_SINGLETON_FACTORIES: RegisteredInstances = OnceCell::const_new();
pub(crate) static REGISTERED_TRANSIENT_FACTORIES: RegisteredInstances = OnceCell::const_new();
pub(crate) static REGISTERED_SCOPE_FACTORIES: RegisteredInstances = OnceCell::const_new();

pub(crate) static SINGLETON_CACHE: once_cell::sync::OnceCell<
    DashMap<(TypeId, String), ServiceInstance>,
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
    let type_id = TypeId::of::<T>();
    let name_string = name.to_string();
    let factories_arcswap = registry
        .get_or_init(|| async { ArcSwap::from_pointee(DashMap::new()) })
        .await;

    let key = (type_id, name_string.clone());

    if factories_arcswap.load().contains_key(&key) {
        return Err(DiError::ServiceAlreadyRegistered(name_string));
    }

    let arc_factory = Arc::new(factory);

    let wrapped_factory = Arc::new(move |scope: Arc<DIScope>| {
        let factory_cloned = arc_factory.clone();
        Box::pin(async move {
            let service = factory_cloned(scope).await?;
            Ok(Arc::new(TokioRwLock::new(service)) as ServiceInstance)
        }) as Pin<Box<dyn Future<Output = Result<ServiceInstance, DiError>> + Send>>
    });

    let current_map = factories_arcswap.load();
    let new_map = DashMap::new();
    for entry in current_map.iter() {
        new_map.insert(entry.key().clone(), entry.value().clone());
    }

    new_map.insert(key, wrapped_factory);
    factories_arcswap.store(Arc::new(new_map));

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
    use crate::DIScope;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[derive(Default)]
    struct TestService;

    #[tokio::test]
    async fn test_register_singleton_and_resolve() {
        register_singleton::<TestService, _, _>(|_| async { Ok(TestService) })
            .await
            .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let resolved = scope.get::<TestService>().await.unwrap();
            let guard = resolved.read().await;
            let ptr = &*guard as *const TestService;
            assert!(!ptr.is_null());
        })
        .await;
    }

    #[tokio::test]
    async fn test_register_singleton_duplicate_should_fail() {
        let _ = register_singleton_name::<TestService, _, _>("duplicate", |_| async {
            Ok(TestService)
        })
        .await;

        let result = register_singleton_name::<TestService, _, _>("duplicate", |_| async {
            Ok(TestService)
        })
        .await;

        assert!(matches!(result, Err(DiError::ServiceAlreadyRegistered(_))));
    }

    #[tokio::test]
    async fn test_register_transient_returns_new_instance() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        #[derive(Default)]
        struct CounterService(usize);

        register_transient::<CounterService, _, _>(|_| async {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(CounterService(id))
        })
        .await
        .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let a = scope.clone().get::<CounterService>().await.unwrap();
            let b = scope.clone().get::<CounterService>().await.unwrap();

            let a_id = a.read().await.0;
            let b_id = b.read().await.0;

            assert_ne!(a_id, b_id);
        })
        .await;
    }

    #[tokio::test]
    async fn test_register_scoped_returns_same_instance_within_scope() {
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        #[derive(Default)]
        struct ScopedService(usize);

        register_scope::<ScopedService, _, _>(|_| async {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(ScopedService(id))
        })
        .await
        .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let a = scope.clone().get::<ScopedService>().await.unwrap();
            let b = scope.clone().get::<ScopedService>().await.unwrap();

            let a_id = a.read().await.0;
            let b_id = b.read().await.0;

            assert_eq!(a_id, b_id);
        })
        .await;
    }

    #[tokio::test]
    async fn test_named_registration_and_resolution() {
        #[derive(Default)]
        struct NamedService(&'static str);

        register_singleton_name::<NamedService, _, _>("alpha", |_| async {
            Ok(NamedService("alpha"))
        })
        .await
        .unwrap();

        register_singleton_name::<NamedService, _, _>("beta", |_| async {
            Ok(NamedService("beta"))
        })
        .await
        .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let alpha = scope
                .clone()
                .get_by_name::<NamedService>("alpha")
                .await
                .unwrap();
            let beta = scope
                .clone()
                .get_by_name::<NamedService>("beta")
                .await
                .unwrap();

            assert_eq!(alpha.read().await.0, "alpha");
            assert_eq!(beta.read().await.0, "beta");
        })
        .await;
    }
}
