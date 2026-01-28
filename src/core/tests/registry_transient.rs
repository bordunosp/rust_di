use crate::core::error_di::DiError;
use crate::core::factory::DiFactory;
use crate::{DIScope, initialize};
use std::sync::Arc;

#[derive(Default)]
struct TransientDefaultService {}

#[rust_di::registry(Transient)]
impl TransientDefaultService {}

#[tokio::test]
async fn test_transient_returns_new_instance_each_time() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let a = scope
            .clone()
            .get::<TransientDefaultService>()
            .await
            .unwrap();
        let b = scope
            .clone()
            .get::<TransientDefaultService>()
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&a, &b));
    })
    .await;
}

#[derive(Default)]
struct TransientNamedService {}

#[rust_di::registry(Transient(name = "custom"))]
impl TransientNamedService {}

#[tokio::test]
async fn test_transient_named_always_gives_new_instance() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let a = scope
            .clone()
            .get_by_name::<TransientNamedService>("custom")
            .await
            .unwrap();
        let b = scope
            .clone()
            .get_by_name::<TransientNamedService>("custom")
            .await
            .unwrap();
        assert!(!Arc::ptr_eq(&a, &b));
    })
    .await;
}

struct TransientFactoryService {
    pub value: &'static str,
}

#[async_trait::async_trait]
impl DiFactory for TransientFactoryService {
    async fn create(_: Arc<DIScope>) -> Result<Self, DiError> {
        Ok(TransientFactoryService { value: "factory" })
    }
}

#[rust_di::registry(Transient(factory = TransientFactoryService))]
impl TransientFactoryService {}

#[tokio::test]
async fn test_transient_factory_registration() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let a = scope
            .clone()
            .get::<TransientFactoryService>()
            .await
            .unwrap();
        let b = scope
            .clone()
            .get::<TransientFactoryService>()
            .await
            .unwrap();
        assert_ne!(a.value, "");
        assert!(!Arc::ptr_eq(&a, &b));
    })
    .await;
}

struct TransientAutoFactoryService {
    pub value: &'static str,
}

#[async_trait::async_trait]
impl DiFactory for TransientAutoFactoryService {
    async fn create(_: Arc<DIScope>) -> Result<Self, DiError> {
        Ok(TransientAutoFactoryService {
            value: "auto factory",
        })
    }
}

#[rust_di::registry(Transient(factory))]
impl TransientAutoFactoryService {}

#[tokio::test]
async fn test_transient_auto_factory_registration() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let a = scope
            .clone()
            .get::<TransientAutoFactoryService>()
            .await
            .unwrap();
        let b = scope
            .clone()
            .get::<TransientAutoFactoryService>()
            .await
            .unwrap();
        assert_eq!(a.value, "auto factory");
        assert!(!Arc::ptr_eq(&a, &b));
    })
    .await;
}
