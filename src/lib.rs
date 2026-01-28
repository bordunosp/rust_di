#![doc = "Dependency Injection framework for Rust"]
extern crate self as rust_di;

pub mod core;

pub use inventory;

inventory::collect!(DiConstructor);

use crate::core::contracts::{ScopedMap, ServiceInstance};
use crate::core::di_inventory::DiConstructor;
use crate::core::error_di::DiError;
use crate::core::registry::{
    REGISTERED_SCOPE_FACTORIES, REGISTERED_SINGLETON_FACTORIES, REGISTERED_TRANSIENT_FACTORIES,
    SINGLETON_CACHE,
};
use dashmap::DashMap;
use std::{cell::RefCell, fmt, future::Future, sync::Arc};

pub use di_macros::main;
/// Attribute macro for registering services.
///
/// # Usage
///
/// ```ignore
/// #[rust_di::registry(
///     Singleton,
///     Singleton(factory),
///     Singleton(name = "custom"),
///     Singleton(factory = MyFactory, name = "custom"),
///
///     Transient,
///     Transient(factory),
///     Transient(name = "custom"),
///     Transient(factory = MyFactory, name = "custom"),
///
///     Scoped,
///     Scoped(factory),
///     Scoped(name = "custom"),
///     Scoped(factory = MyFactory, name = "custom"),
/// )]
/// impl MyService {}
/// ```
pub use di_macros::registry;
pub use di_macros::with_di_scope;

use tokio::sync::OnceCell;

static INIT: OnceCell<()> = OnceCell::const_new();

pub async fn initialize() {
    INIT.get_or_init(|| async {
        for ctor in inventory::iter::<DiConstructor> {
            (ctor.init)().await;
        }
    })
    .await;
}

tokio::task_local! {
    static CURRENT_DI_SCOPE: Arc<DIScope>;
    static RESOLVING_STACK: RefCell<Vec<String>>;
}

pub struct DIScope {
    pub scoped_instances: Arc<ScopedMap>,
}

impl Drop for DIScope {
    fn drop(&mut self) {
        self.scoped_instances.clear();
    }
}

impl fmt::Debug for DIScope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("DIScope")
            .field("scoped_instances_count", &self.scoped_instances.len())
            .finish()
    }
}

impl DIScope {
    pub async fn new() -> Arc<Self> {
        REGISTERED_SINGLETON_FACTORIES
            .get_or_init(|| async { arc_swap::ArcSwap::from_pointee(DashMap::new()) })
            .await;
        REGISTERED_TRANSIENT_FACTORIES
            .get_or_init(|| async { arc_swap::ArcSwap::from_pointee(DashMap::new()) })
            .await;
        REGISTERED_SCOPE_FACTORIES
            .get_or_init(|| async { arc_swap::ArcSwap::from_pointee(DashMap::new()) })
            .await;

        Arc::new(DIScope {
            scoped_instances: Arc::new(DashMap::new()),
        })
    }

    pub fn current() -> Result<Arc<DIScope>, DiError> {
        CURRENT_DI_SCOPE
            .try_with(|scope| scope.clone())
            .map_err(|e| {
                DiError::FactoryError(Box::new(std::io::Error::other(format!(
                    "No DI scope found in this task: {e}",
                ))))
            })
    }

    pub async fn run_with_scope<F, RFut, ROutput>(func: F) -> ROutput
    where
        F: FnOnce() -> RFut,
        RFut: Future<Output = ROutput>,
    {
        let scope = DIScope::new().await;
        RESOLVING_STACK
            .scope(RefCell::new(Vec::new()), async {
                CURRENT_DI_SCOPE.scope(scope.clone(), func()).await
            })
            .await
    }

    pub async fn get<T>(self: Arc<Self>) -> Result<Arc<T>, DiError>
    where
        T: Send + Sync + 'static,
    {
        self.get_by_name::<T>("").await
    }

    pub async fn get_by_name<T>(self: Arc<Self>, name: &str) -> Result<Arc<T>, DiError>
    where
        T: Send + Sync + 'static,
    {
        let type_key = std::any::type_name::<T>().to_string();
        let name_string = name.to_string();
        let key = (type_key.clone(), name_string.clone());

        // Захист від циклічних залежностей
        RESOLVING_STACK
            .try_with(|stack| {
                let mut stack_ref = stack.borrow_mut();
                if stack_ref.contains(&type_key) {
                    return Err(DiError::CircularDependency(name_string.clone()));
                }
                stack_ref.push(type_key.clone());
                Ok(())
            })
            .map_err(|e| {
                DiError::FactoryError(Box::new(std::io::Error::other(format!(
                    "Failed to access resolving stack: {e}",
                ))))
            })??;

        let result: Result<ServiceInstance, DiError> = async {
            // 🔁 Scoped (в межах поточного DIScope)
            {
                if let Some(entry) = self.scoped_instances.get(&key) {
                    return Ok(entry.value().clone());
                }
                if let Some(factories) = REGISTERED_SCOPE_FACTORIES.get()
                    && let Some(factory) = factories.load().get(&key)
                {
                    let instance = factory.value()(self.clone()).await?;
                    self.scoped_instances.insert(key.clone(), instance.clone());
                    return Ok(instance);
                }
            }

            // 🔁 Singleton (глобальний кеш)
            {
                if let Some(factories) = REGISTERED_SINGLETON_FACTORIES.get()
                    && let Some(factory) = factories.load().get(&key)
                {
                    let cache = SINGLETON_CACHE.get_or_init(DashMap::new);
                    if let Some(cached) = cache.get(&key) {
                        return Ok(cached.value().clone());
                    }
                    let instance = factory.value()(self.clone()).await?;
                    cache.insert(key.clone(), instance.clone());
                    return Ok(instance);
                }
            }

            // 🔁 Transient (новий кожного разу)
            {
                if let Some(factories) = REGISTERED_TRANSIENT_FACTORIES.get()
                    && let Some(factory) = factories.load().get(&key)
                {
                    let instance = factory.value()(self.clone()).await?;
                    return Ok(instance);
                }
            }

            Err(DiError::ServiceNotFound(name_string.clone()))
        }
        .await;

        // Знімаємо з resolving stack
        RESOLVING_STACK
            .try_with(|stack| {
                stack.borrow_mut().pop();
                Ok(())
            })
            .map_err(|e| {
                DiError::FactoryError(Box::new(std::io::Error::other(format!(
                    "Failed to access resolving stack: {e}",
                ))))
            })??;

        result.and_then(|instance| {
            let any_instance: Arc<dyn std::any::Any + Send + Sync> = instance;

            any_instance.downcast::<T>().map_err(|_| {
                DiError::FactoryError(Box::new(std::io::Error::other(format!(
                    "Type mismatch: could not downcast to {}",
                    std::any::type_name::<T>()
                ))))
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DIScope;
    use crate::core::registry::register_singleton_name;
    use crate::core::registry::*;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

    #[derive(Default)]
    struct FlagService;

    #[derive(Default)]
    struct SimpleService;

    static FLAG: AtomicBool = AtomicBool::new(false);

    #[registry(Singleton)]
    impl FlagService {}

    #[with_di_scope]
    async fn scoped_entry() {
        initialize().await;
        let scope = DIScope::current().unwrap();
        let _svc = scope.get::<FlagService>().await.unwrap();
        FLAG.store(true, Ordering::SeqCst);
    }

    #[tokio::test]
    async fn test_with_di_scope_macro_executes_in_scope() {
        initialize().await;
        let _ = scoped_entry().await;
        assert!(
            FLAG.load(Ordering::SeqCst),
            "Service was not resolved inside DI scope"
        );
    }

    #[tokio::test]
    async fn test_singleton_resolves_once() {
        initialize().await;
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        register_singleton::<SimpleService, _, _>(|_| async {
            COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(SimpleService)
        })
        .await
        .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let a = scope.clone().get::<SimpleService>().await.unwrap();
            let b = scope.clone().get::<SimpleService>().await.unwrap();
            assert!(std::ptr::eq(Arc::as_ptr(&a), Arc::as_ptr(&b)));
        })
        .await;

        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_scoped_resolves_once_per_scope() {
        initialize().await;
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
            assert_eq!(a.0, b.0);
        })
        .await;

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let c = scope.get::<ScopedService>().await.unwrap();
            assert_ne!(c.0, 0);
        })
        .await;
    }

    #[tokio::test]
    async fn test_transient_resolves_new_each_time() {
        initialize().await;
        static COUNTER: AtomicUsize = AtomicUsize::new(0);

        #[derive(Default)]
        struct TransientService(usize);

        register_transient::<TransientService, _, _>(|_| async {
            let id = COUNTER.fetch_add(1, Ordering::SeqCst);
            Ok(TransientService(id))
        })
        .await
        .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let a = scope.clone().get::<TransientService>().await.unwrap();
            let b = scope.clone().get::<TransientService>().await.unwrap();
            assert_ne!(a.0, b.0);
        })
        .await;
    }

    #[tokio::test]
    async fn test_named_instances_resolve_independently() {
        initialize().await;
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

            assert_eq!(alpha.0, "alpha");
            assert_eq!(beta.0, "beta");
        })
        .await;
    }

    #[tokio::test]
    async fn test_circular_dependency_detection() {
        initialize().await;
        #[derive(Default)]
        struct A;
        #[derive(Default)]
        struct B;

        register_transient::<A, _, _>(|scope| async {
            let _ = scope.get::<B>().await?;
            Ok(A)
        })
        .await
        .unwrap();

        register_transient::<B, _, _>(|scope| async {
            let _ = scope.get::<A>().await?;
            Ok(B)
        })
        .await
        .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let result = scope.get::<A>().await;
            assert!(matches!(result, Err(DiError::CircularDependency(_))));
        })
        .await;
    }

    #[tokio::test]
    async fn test_scope_drop_clears_instances() {
        initialize().await;
        use std::sync::atomic::{AtomicBool, Ordering};

        static DROPPED: AtomicBool = AtomicBool::new(false);

        struct DroppableService;

        impl Drop for DroppableService {
            fn drop(&mut self) {
                DROPPED.store(true, Ordering::SeqCst);
            }
        }

        register_scope::<DroppableService, _, _>(|_| async { Ok(DroppableService) })
            .await
            .unwrap();

        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let _instance = scope.get::<DroppableService>().await.unwrap();
            assert!(!DROPPED.load(Ordering::SeqCst), "Service dropped too early");
        })
        .await;

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        assert!(
            DROPPED.load(Ordering::SeqCst),
            "Scoped instance was not dropped"
        );
    }
}
