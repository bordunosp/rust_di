use super::*;
use dashmap::DashMap;
use std::sync::atomic::Ordering;

pub static TEST_SERVICE_COUNTER: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub async fn reset_global_di_state_for_tests() -> Result<(), DiError> {
    let singletons = REGISTERED_SINGLETON_INSTANCES
        .get_or_init(|| async { ArcSwap::from_pointee(DashMap::new()) })
        .await;
    singletons.store(Arc::new(DashMap::new()));

    let transients = REGISTERED_TRANSIENT_FACTORIES
        .get_or_init(|| async { ArcSwap::from_pointee(DashMap::new()) })
        .await;
    transients.store(Arc::new(DashMap::new()));

    let scopes = REGISTERED_SCOPE_FACTORIES
        .get_or_init(|| async { ArcSwap::from_pointee(DashMap::new()) })
        .await;
    scopes.store(Arc::new(DashMap::new()));

    GLOBAL_SERVICE_COUNTER.store(0, Ordering::SeqCst);
    TEST_SERVICE_COUNTER.store(0, Ordering::SeqCst);
    Ok(())
}
