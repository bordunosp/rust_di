use arc_swap::ArcSwap;
use dashmap::DashMap; // Використовуємо DashMap
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::Arc,
};
use tokio::sync::{OnceCell, RwLock as TokioRwLock};

// --- Типи та статичні реєстри ---

type RegisteredInstances = OnceCell<
    ArcSwap<
        DashMap<
            (TypeId, String),
            Arc<
                dyn Fn(
                    Arc<DIScope>,
                ) -> Pin<
                    Box<
                        dyn Future<
                            Output = Result<
                                Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>,
                                DiError,
                            >,
                        > + Send,
                    >,
                > + Send
                + Sync,
            >,
        >,
    >,
>;

static REGISTERED_SINGLETON_INSTANCES: OnceCell<
    ArcSwap<DashMap<(TypeId, String), Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>>>, // Замінено HashMap на DashMap
> = OnceCell::const_new();
static REGISTERED_TRANSIENT_FACTORIES: RegisteredInstances = OnceCell::const_new();
static REGISTERED_SCOPE_FACTORIES: RegisteredInstances = OnceCell::const_new();

pub static GLOBAL_SERVICE_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

// --- DiError та його імплементації ---

#[derive(Debug)]
pub enum DiError {
    ServiceNotFound(TypeId, String),
    ServiceAlreadyRegistered(TypeId, String),
    LockPoisoned,
    FactoryError(Box<dyn Error + Send + Sync + 'static>),
    CircularDependency(TypeId, String),
}

impl fmt::Display for DiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DiError::ServiceNotFound(type_id, name) => {
                write!(f, "Service not found for TypeId: {:?} with name: {}", type_id, name)
            }
            DiError::ServiceAlreadyRegistered(type_id, name) => {
                write!(f, "Service already registered for TypeId: {:?} with name: {}", type_id, name)
            }
            DiError::LockPoisoned => write!(f, "A Mutex or RwLock was poisoned"),
            DiError::FactoryError(err) => write!(f, "Service factory error: {}", err),
            DiError::CircularDependency(type_id, name) => {
                write!(f, "Circular dependency detected for TypeId: {:?} with name: {}", type_id, name)
            }
        }
    }
}

impl Error for DiError {}

pub trait AnyService: Any + Send + Sync + 'static {}
impl<T: Any + Send + Sync + 'static> AnyService for T {}

// --- Допоміжні функції для реєстрації (усунення дублювання) ---

async fn register_factory<T, F, Fut>(
    name: &str,
    factory: Arc<F>,
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
        .get_or_init(|| async { ArcSwap::from_pointee(DashMap::new()) }) // Ініціалізація DashMap
        .await;

    let key = (type_id, name_string.clone());

    if factories_arcswap.load().contains_key(&key) {
        return Err(DiError::ServiceAlreadyRegistered(type_id, name_string));
    }

    let wrapped_factory = Arc::new(move |scope: Arc<DIScope>| {
        let factory_cloned = factory.clone();
        Box::pin(async move {
            let service = factory_cloned(scope).await?;
            Ok(Arc::new(TokioRwLock::new(service))
                as Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>)
        }) as Pin<
            Box<
                dyn Future<
                    Output = Result<
                        Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>,
                        DiError,
                    >,
                > + Send,
            >,
        >
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

// --- Публічні функції реєстрації ---

pub async fn register_transient<T, F, Fut>(factory: F) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    register_factory("", Arc::new(factory), &REGISTERED_TRANSIENT_FACTORIES).await
}

pub async fn register_transient_name<T, F, Fut>(name: &str, factory: Arc<F>) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    register_factory(name, factory, &REGISTERED_TRANSIENT_FACTORIES).await
}

pub async fn register_scope<T, F, Fut>(factory: F) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    register_factory("", Arc::new(factory), &REGISTERED_SCOPE_FACTORIES).await
}

pub async fn register_scope_name<T, F, Fut>(name: &str, factory: Arc<F>) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
    F: Fn(Arc<DIScope>) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<T, DiError>> + Send + 'static,
{
    register_factory(name, factory, &REGISTERED_SCOPE_FACTORIES).await
}

pub async fn register_singleton<T>(instance: T) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
{
    register_singleton_name("", instance).await
}

pub async fn register_singleton_name<T>(name: &str, instance: T) -> Result<(), DiError>
where
    T: Send + Sync + 'static,
{
    let type_id = TypeId::of::<T>();
    let name_string = name.to_string();
    let instances_arcswap = REGISTERED_SINGLETON_INSTANCES
        .get_or_init(|| async { ArcSwap::from_pointee(DashMap::new()) }) // Ініціалізація DashMap
        .await;

    let key = (type_id, name_string.clone());

    if instances_arcswap.load().contains_key(&key) {
        return Err(DiError::ServiceAlreadyRegistered(type_id, name_string));
    }

    let current_map = instances_arcswap.load();
    let new_map = DashMap::new();
    for entry in current_map.iter() {
        new_map.insert(entry.key().clone(), entry.value().clone());
    }
    new_map.insert(
        key,
        Arc::new(TokioRwLock::new(instance))
            as Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>,
    );
    
    instances_arcswap.store(Arc::new(new_map));
    Ok(())
}

tokio::task_local! {
    static CURRENT_DI_SCOPE: Arc<DIScope>;
    static RESOLVING_STACK: RefCell<Vec<TypeId>>;
}

// --- DIScope та його імплементації ---

pub struct DIScope {
    singleton_instances: &'static ArcSwap<
        DashMap<(TypeId, String), Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>>, // Замінено HashMap на DashMap
    >,
    transient_factories: &'static ArcSwap<
        DashMap<
            (TypeId, String),
            Arc<
                dyn Fn(
                    Arc<DIScope>,
                ) -> Pin<
                    Box<
                        dyn Future<
                            Output = Result<
                                Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>,
                                DiError,
                            >,
                        > + Send,
                    >,
                > + Send
                + Sync,
            >,
        >,
    >,

    scope_factories: &'static ArcSwap<
        DashMap<
            (TypeId, String),
            Arc<
                dyn Fn(
                    Arc<DIScope>,
                ) -> Pin<
                    Box<
                        dyn Future<
                            Output = Result<
                                Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>,
                                DiError,
                            >,
                        > + Send,
                    >,
                > + Send
                + Sync,
            >,
        >,
    >,
    pub scoped_instances:
        Arc<DashMap<(TypeId, String), Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>>>,
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
        let singleton_instances = REGISTERED_SINGLETON_INSTANCES
            .get_or_init(|| async { ArcSwap::from_pointee(DashMap::new()) })
            .await;

        let transient_factories = REGISTERED_TRANSIENT_FACTORIES
            .get_or_init(|| async { ArcSwap::from_pointee(DashMap::new()) })
            .await;

        let scope_factories = REGISTERED_SCOPE_FACTORIES
            .get_or_init(|| async { ArcSwap::from_pointee(DashMap::new()) })
            .await;

        Arc::new(DIScope {
            singleton_instances,
            transient_factories,
            scope_factories,
            scoped_instances: Arc::new(DashMap::new()),
        })
    }

    pub fn current() -> Result<Arc<DIScope>, DiError> {
        CURRENT_DI_SCOPE
            .try_with(|scope| scope.clone())
            .map_err(|e| DiError::FactoryError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("No DI scope found in this task: {}", e)))))
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

    pub async fn get<T>(self: Arc<Self>) -> Result<Arc<TokioRwLock<T>>, DiError>
    where
        T: Send + Sync + 'static,
    {
        self.by_name::<T>("").await
    }

    pub async fn by_name<T>(self: Arc<Self>, name: &str) -> Result<Arc<TokioRwLock<T>>, DiError>
    where
        T: Send + Sync + 'static,
    {
        let type_id = TypeId::of::<T>();
        let name_string = name.to_string();
        let key = (type_id, name_string.clone());

        let _ = RESOLVING_STACK
            .try_with(|stack| {
                let mut stack_ref = stack.borrow_mut();
                if stack_ref.contains(&type_id) {
                    return Err(DiError::CircularDependency(type_id, name_string.clone()));
                }
                stack_ref.push(type_id);
                Ok::<(), DiError>(())
            })
            .map_err(|e| {
                DiError::FactoryError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to access resolving stack (AccessError): {}", e))))
            })?;

        let result: Result<Arc<TokioRwLock<dyn AnyService + Send + Sync + 'static>>, DiError> =
            async {
                // Try singletons
                {
                    let singletons_guard = self.singleton_instances.load();
                    if let Some(instance) = singletons_guard.get(&key) {
                        return Ok(instance.value().clone());
                    }
                }

                // Try scoped instances
                {
                    if let Some(entry) = self.scoped_instances.get(&(type_id, name_string.clone()))
                    {
                        return Ok(entry.value().clone());
                    }
                }

                // Try scope factories
                {
                    let factories_guard = self.scope_factories.load();
                    if let Some(factory) = factories_guard.get(&(type_id, name_string.clone())) {
                        let instance = factory.value()(self.clone()).await?;
                        self.scoped_instances
                            .insert((type_id, name_string.clone()), instance.clone());
                        return Ok(instance.clone());
                    }
                }

                // Try transient factories
                {
                    let factories_guard = self.transient_factories.load();
                    if let Some(factory) = factories_guard.get(&(type_id, name_string.clone())) {
                        let instance = factory.value()(self.clone()).await?;
                        return Ok(instance.clone());
                    }
                }

                Err(DiError::ServiceNotFound(type_id, name_string.clone()))
            }
                .await;

        let _ = RESOLVING_STACK
            .try_with(|stack| {
                stack.borrow_mut().pop();
                Ok::<(), DiError>(())
            })
            .map_err(|e| {
                DiError::FactoryError(Box::new(std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to access resolving stack (AccessError): {}", e))))
            })?;

        result.map(|instance| {
            let raw_ptr: *const TokioRwLock<dyn AnyService + Send + Sync + 'static> =
                Arc::into_raw(instance);
            let typed_raw_ptr: *const TokioRwLock<T> = raw_ptr.cast();
            unsafe { Arc::from_raw(typed_raw_ptr) }
        })
    }

    pub async fn clear_scoped_instances(&self) -> Result<(), DiError> {
        self.scoped_instances.clear();
        Ok(())
    }
}

// --- Тести ---
#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static COUNTER: AtomicUsize = AtomicUsize::new(0);

    pub async fn reset_global_di_state() -> Result<(), DiError> {
        if let Some(registry) = REGISTERED_SINGLETON_INSTANCES.get() {
            registry.store(Arc::new(DashMap::new()));
        }
        if let Some(registry) = REGISTERED_TRANSIENT_FACTORIES.get() {
            registry.store(Arc::new(DashMap::new()));
        }
        if let Some(registry) = REGISTERED_SCOPE_FACTORIES.get() {
            registry.store(Arc::new(DashMap::new()));
        }
        GLOBAL_SERVICE_COUNTER.store(0, Ordering::SeqCst);
        COUNTER.store(0, Ordering::SeqCst);
        Ok(())
    }

    #[derive(Debug)]
    struct TestServiceA {
        id: usize,
    }
    impl TestServiceA {
        fn new() -> Self {
            TestServiceA {
                id: COUNTER.fetch_add(1, Ordering::SeqCst),
            }
        }
    }

    #[derive(Debug)]
    struct TestServiceB {
        id: usize,
    }
    impl TestServiceB {
        fn new() -> Self {
            TestServiceB {
                id: COUNTER.fetch_add(1, Ordering::SeqCst),
            }
        }
    }

    #[derive(Debug)]
    struct TestServiceC {
        id: usize,
    }
    impl TestServiceC {
        fn new() -> Self {
            TestServiceC {
                id: COUNTER.fetch_add(1, Ordering::SeqCst),
            }
        }
    }

    #[derive(Debug)]
    struct TestServiceD {
        id: usize,
    }
    impl TestServiceD {
        fn new() -> Self {
            TestServiceD {
                id: COUNTER.fetch_add(1, Ordering::SeqCst),
            }
        }
    }

    #[derive(Debug)]
    struct TestServiceE {
        id: usize,
    }
    impl TestServiceE {
        fn new() -> Self {
            TestServiceE {
                id: COUNTER.fetch_add(1, Ordering::SeqCst),
            }
        }
    }

    #[derive(Debug)]
    struct MixedServiceA {
        id: usize,
    }
    impl MixedServiceA {
        fn new() -> Self {
            MixedServiceA {
                id: COUNTER.fetch_add(1, Ordering::SeqCst),
            }
        }
    }
    #[derive(Debug)]
    struct MixedServiceB {
        id: usize,
    }
    impl MixedServiceB {
        fn new() -> Self {
            MixedServiceB {
                id: COUNTER.fetch_add(1, Ordering::SeqCst),
            }
        }
    }
    #[derive(Debug)]
    struct MixedServiceC {
        id: usize,
    }
    impl MixedServiceC {
        fn new() -> Self {
            MixedServiceC {
                id: COUNTER.fetch_add(1, Ordering::SeqCst),
            }
        }
    }

    #[derive(Debug)]
    struct MissingService;
    #[derive(Debug)]
    struct OverlappingService;
    #[derive(Debug)]
    struct ScopedClearService;
    impl ScopedClearService {
        fn new() -> Self {
            ScopedClearService
        }
    }

    #[derive(Debug)]
    struct NamedSingletonService {
        id: usize,
        name: String,
    }
    impl NamedSingletonService {
        fn new(name: &str) -> Self {
            NamedSingletonService {
                id: COUNTER.fetch_add(1, Ordering::SeqCst),
                name: name.to_string(),
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_transient_service() {
        reset_global_di_state().await.unwrap();

        register_transient(|_| async move { Ok(TestServiceA::new()) })
            .await
            .unwrap();

        DIScope::run_with_scope(|| async {
            let resolver = DIScope::current().unwrap();

            let service1 = resolver.clone().get::<TestServiceA>().await.unwrap();
            let service2 = resolver.clone().get::<TestServiceA>().await.unwrap();

            assert_ne!(service1.read().await.id, service2.read().await.id);
            assert_eq!(COUNTER.load(Ordering::SeqCst), 2);
            Ok::<(), DiError>(())
        })
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_scoped_service() {
        reset_global_di_state().await.unwrap();

        register_scope(|_| async move { Ok(TestServiceB::new()) })
            .await
            .unwrap();

        DIScope::run_with_scope(|| async {
            let resolver = DIScope::current().unwrap();

            let service1 = resolver.clone().get::<TestServiceB>().await.unwrap();
            let service2 = resolver.clone().get::<TestServiceB>().await.unwrap();

            assert_eq!(service1.read().await.id, service2.read().await.id);
            assert_eq!(COUNTER.load(Ordering::SeqCst), 1);

            register_scope_name(
                "named_scoped_service",
                Arc::new(|_| async move { Ok(TestServiceC::new()) }),
            )
                .await
                .unwrap();
            let named_service = resolver
                .clone()
                .by_name::<TestServiceC>("named_scoped_service")
                .await
                .unwrap();
            assert_eq!(named_service.read().await.id, 1);
            assert_eq!(COUNTER.load(Ordering::SeqCst), 2);
            Ok::<(), DiError>(())
        })
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_scoped_service_different_scopes() {
        reset_global_di_state().await.unwrap();

        register_scope(|_| async move { Ok(TestServiceD::new()) })
            .await
            .unwrap();

        let service_id_scope1 = DIScope::run_with_scope(|| async {
            let resolver = DIScope::current().unwrap();
            let service = resolver.get::<TestServiceD>().await.unwrap();
            Ok::<usize, DiError>(service.read().await.id)
        })
            .await
            .unwrap();

        let service_id_scope2 = DIScope::run_with_scope(|| async {
            let resolver = DIScope::current().unwrap();
            let service = resolver.get::<TestServiceD>().await.unwrap();
            Ok::<usize, DiError>(service.read().await.id)
        })
            .await
            .unwrap();

        assert_ne!(service_id_scope1, service_id_scope2);
        assert_eq!(COUNTER.load(Ordering::SeqCst), 2);
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_singleton_service() {
        reset_global_di_state().await.unwrap();

        register_singleton(TestServiceE::new()).await.unwrap();
        let service_id_at_registration = 0;

        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);

        DIScope::run_with_scope(|| async {
            let resolver = DIScope::current().unwrap();
            let service1 = resolver.clone().get::<TestServiceE>().await.unwrap();
            let service2 = resolver.clone().get::<TestServiceE>().await.unwrap();

            assert_eq!(service1.read().await.id, service2.read().await.id);
            assert_eq!(service1.read().await.id, service_id_at_registration);
            assert_eq!(COUNTER.load(Ordering::SeqCst), 1);
            Ok::<(), DiError>(())
        })
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_multiple_named_singletons_of_same_type() {
        reset_global_di_state().await.unwrap();

        register_singleton_name("db_conn_1", NamedSingletonService::new("db_conn_1"))
            .await
            .unwrap();
        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);

        register_singleton_name("db_conn_2", NamedSingletonService::new("db_conn_2"))
            .await
            .unwrap();
        assert_eq!(COUNTER.load(Ordering::SeqCst), 2);

        DIScope::run_with_scope(|| async {
            let resolver = DIScope::current().unwrap();

            let service1 = resolver
                .clone()
                .by_name::<NamedSingletonService>("db_conn_1")
                .await
                .unwrap();
            assert_eq!(service1.read().await.id, 0);
            assert_eq!(service1.read().await.name, "db_conn_1");

            let service2 = resolver
                .clone()
                .by_name::<NamedSingletonService>("db_conn_2")
                .await
                .unwrap();
            assert_eq!(service2.read().await.id, 1);
            assert_eq!(service2.read().await.name, "db_conn_2");

            assert_ne!(service1.read().await.id, service2.read().await.id);

            let service1_again = resolver
                .clone()
                .by_name::<NamedSingletonService>("db_conn_1")
                .await
                .unwrap();
            assert_eq!(service1.read().await.id, service1_again.read().await.id);
            assert_eq!(COUNTER.load(Ordering::SeqCst), 2);

            Ok::<(), DiError>(())
        })
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_unnamed_and_named_singleton_coexistence() {
        reset_global_di_state().await.unwrap();

        #[derive(Debug)]
        struct CoexistService {
            id: usize,
        }
        impl CoexistService {
            fn new() -> Self {
                CoexistService {
                    id: COUNTER.fetch_add(1, Ordering::SeqCst),
                }
            }
        }

        register_singleton(CoexistService::new()).await.unwrap();
        assert_eq!(COUNTER.load(Ordering::SeqCst), 1);

        register_singleton_name("special_coexist", CoexistService::new())
            .await
            .unwrap();
        assert_eq!(COUNTER.load(Ordering::SeqCst), 2);

        DIScope::run_with_scope(|| async {
            let resolver = DIScope::current().unwrap();

            let default_service = resolver.clone().get::<CoexistService>().await.unwrap();
            assert_eq!(default_service.read().await.id, 0);

            let named_service = resolver
                .clone()
                .by_name::<CoexistService>("special_coexist")
                .await
                .unwrap();
            assert_eq!(named_service.read().await.id, 1);

            assert_ne!(
                default_service.read().await.id,
                named_service.read().await.id
            );
            Ok::<(), DiError>(())
        })
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_mixed_lifetimes() {
        reset_global_di_state().await.unwrap();

        register_singleton(MixedServiceA::new()).await.unwrap();

        register_transient_name(
            "transient_b",
            Arc::new(|_| async move { Ok(MixedServiceB::new()) }),
        )
            .await
            .unwrap();

        register_scope_name(
            "scoped_c",
            Arc::new(|_| async move { Ok(MixedServiceC::new()) }),
        )
            .await
            .unwrap();

        DIScope::run_with_scope(|| async {
            let resolver = DIScope::current().unwrap();

            let s_singleton1 = resolver.clone().get::<MixedServiceA>().await.unwrap();
            let s_singleton2 = resolver.clone().get::<MixedServiceA>().await.unwrap();
            assert_eq!(s_singleton1.read().await.id, s_singleton2.read().await.id);
            assert_eq!(s_singleton1.read().await.id, 0);
            assert_eq!(COUNTER.load(Ordering::SeqCst), 1);

            let t_b1 = resolver
                .clone()
                .by_name::<MixedServiceB>("transient_b")
                .await
                .unwrap();
            let t_b2 = resolver
                .clone()
                .by_name::<MixedServiceB>("transient_b")
                .await
                .unwrap();
            assert_ne!(t_b1.read().await.id, t_b2.read().await.id);
            assert_eq!(COUNTER.load(Ordering::SeqCst), 3);

            let sc_c1 = resolver
                .clone()
                .by_name::<MixedServiceC>("scoped_c")
                .await
                .unwrap();
            let sc_c2 = resolver
                .clone()
                .by_name::<MixedServiceC>("scoped_c")
                .await
                .unwrap();
            assert_eq!(sc_c1.read().await.id, sc_c2.read().await.id);
            assert_eq!(COUNTER.load(Ordering::SeqCst), 4);
            Ok::<(), DiError>(())
        })
            .await
            .unwrap();

        DIScope::run_with_scope(|| async {
            let resolver = DIScope::current().unwrap();

            let sc_c_new_scope = resolver.by_name::<MixedServiceC>("scoped_c").await.unwrap();
            assert_eq!(COUNTER.load(Ordering::SeqCst), 5);
            assert_ne!(sc_c_new_scope.read().await.id, 3);
            assert_eq!(sc_c_new_scope.read().await.id, 4);
            Ok::<(), DiError>(())
        })
            .await
            .unwrap();
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_service_not_found() {
        reset_global_di_state().await.unwrap();

        let result = DIScope::run_with_scope(|| async {
            let resolver = DIScope::current()?;
            let result: Result<Arc<TokioRwLock<MissingService>>, DiError> =
                resolver.get::<MissingService>().await;
            result
        })
            .await;
        assert!(matches!(result, Err(DiError::ServiceNotFound(_, _))));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_service_already_registered() {
        reset_global_di_state().await.unwrap();

        register_transient(|_| async move { Ok(OverlappingService) })
            .await
            .unwrap();

        let result = register_transient(|_| async move { Ok(OverlappingService) }).await;
        assert!(matches!(
            result,
            Err(DiError::ServiceAlreadyRegistered(_, _))
        ));

        reset_global_di_state().await.unwrap();
        register_singleton(TestServiceA::new()).await.unwrap();
        let result = register_singleton(TestServiceA::new()).await;
        assert!(matches!(
            result,
            Err(DiError::ServiceAlreadyRegistered(_, _))
        ));

        reset_global_di_state().await.unwrap();
        register_singleton_name("my_named_svc", TestServiceA::new())
            .await
            .unwrap();
        let result = register_singleton_name("my_named_svc", TestServiceA::new()).await;
        assert!(matches!(
            result,
            Err(DiError::ServiceAlreadyRegistered(_, _))
        ));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_no_scope_found() {
        reset_global_di_state().await.unwrap();

        let result = DIScope::current();
        assert!(matches!(result, Err(DiError::FactoryError(_))));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_clear_scope_services() {
        reset_global_di_state().await.unwrap();

        register_scope(|_| async move { Ok(ScopedClearService::new()) })
            .await
            .unwrap();

        DIScope::run_with_scope(|| async {
            let resolver = DIScope::current().unwrap();
            let _ = resolver.clone().get::<ScopedClearService>().await.unwrap();

            assert!(
                resolver
                    .scoped_instances
                    .contains_key(&(TypeId::of::<ScopedClearService>(), "".to_string()))
            );

            resolver.clear_scoped_instances().await.unwrap();

            assert!(resolver.scoped_instances.is_empty());

            let _ = resolver.clone().get::<ScopedClearService>().await.unwrap();
            assert!(
                resolver
                    .scoped_instances
                    .contains_key(&(TypeId::of::<ScopedClearService>(), "".to_string()))
            );
            Ok::<(), DiError>(())
        })
            .await
            .unwrap();
    }
}