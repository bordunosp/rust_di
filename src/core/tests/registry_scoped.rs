use crate::core::error_di::DiError;
use crate::core::factory::DiFactory;
use crate::{DIScope, initialize};
use std::sync::Arc;

#[derive(Default)]
struct ScopedDefaultService {}

#[rust_di::registry(Scoped)]
impl ScopedDefaultService {}

#[tokio::test]
async fn test_scoped_default_registration() {
    initialize().await;
    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let instance1 = scope.clone().get::<ScopedDefaultService>().await.unwrap();
        let instance2 = scope.clone().get::<ScopedDefaultService>().await.unwrap();
        assert!(Arc::ptr_eq(&instance1, &instance2));
    })
    .await;
}

#[derive(Default)]
struct ScopedNamedService {
    pub value: &'static str,
}

#[rust_di::registry(Scoped(name = "named"))]
impl ScopedNamedService {}

#[tokio::test]
async fn test_scoped_named_registration() {
    initialize().await;
    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();

        {
            let instance = scope
                .clone()
                .get_by_name::<ScopedNamedService>("named")
                .await
                .unwrap();
            let mut srv = instance.write().await;
            srv.value = "updated named";
        }

        let value = scope
            .clone()
            .get_by_name::<ScopedNamedService>("named")
            .await
            .unwrap()
            .read()
            .await
            .value;

        assert_eq!(value, "updated named");
    })
    .await;
}

struct ScopedFactoryService {
    pub value: &'static str,
}

#[async_trait::async_trait]
impl DiFactory for ScopedFactoryService {
    async fn create(_: Arc<DIScope>) -> Result<Self, DiError> {
        Ok(ScopedFactoryService {
            value: "scoped factory",
        })
    }
}

#[rust_di::registry(Scoped(factory = ScopedFactoryService))]
impl ScopedFactoryService {}

#[tokio::test]
async fn test_scoped_factory_registration() {
    initialize().await;
    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let instance = scope.get::<ScopedFactoryService>().await.unwrap();
        let value = instance.read().await.value;
        assert_eq!(value, "scoped factory");
    })
    .await;
}

struct ScopedAutoFactoryService {
    pub value: &'static str,
}

#[async_trait::async_trait]
impl DiFactory for ScopedAutoFactoryService {
    async fn create(_: Arc<DIScope>) -> Result<Self, DiError> {
        Ok(ScopedAutoFactoryService {
            value: "auto scoped",
        })
    }
}

#[rust_di::registry(Scoped(factory))]
impl ScopedAutoFactoryService {}

#[tokio::test]
async fn test_scoped_auto_factory_registration() {
    initialize().await;
    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let instance = scope.get::<ScopedAutoFactoryService>().await.unwrap();
        let value = instance.read().await.value;
        assert_eq!(value, "auto scoped");
    })
    .await;
}

#[derive(Default)]
struct ScopedIsolatedService {
    pub value: &'static str,
}

#[rust_di::registry(Scoped)]
impl ScopedIsolatedService {}

#[tokio::test]
async fn test_scoped_returns_new_instance_for_each_scope() {
    initialize().await;
    let mut values = Vec::new();

    for v in &["a", "b"] {
        DIScope::run_with_scope(|| async {
            let scope = DIScope::current().unwrap();
            let instance = scope.clone().get::<ScopedIsolatedService>().await.unwrap();
            let mut srv = instance.write().await;
            srv.value = v;
            values.push(scope.clone().get::<ScopedIsolatedService>().await.unwrap());
        })
        .await;
    }

    let val_1 = values[0].read().await.value;
    let val_2 = values[1].read().await.value;

    assert_ne!(val_1, val_2);
}

#[derive(Default)]
struct ScopedMultiNamedService {
    pub value: &'static str,
}

#[rust_di::registry(Scoped(name = "first"))]
#[rust_di::registry(Scoped(name = "second"))]
impl ScopedMultiNamedService {}

#[tokio::test]
async fn test_scoped_multiple_named_instances_are_distinct() {
    initialize().await;
    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();

        {
            let first = scope
                .clone()
                .get_by_name::<ScopedMultiNamedService>("first")
                .await
                .unwrap();
            let mut srv = first.write().await;
            srv.value = "first scoped";
        }

        {
            let second = scope
                .clone()
                .get_by_name::<ScopedMultiNamedService>("second")
                .await
                .unwrap();
            let mut srv = second.write().await;
            srv.value = "second scoped";
        }

        let first_val = scope
            .clone()
            .get_by_name::<ScopedMultiNamedService>("first")
            .await
            .unwrap()
            .read()
            .await
            .value;

        let second_val = scope
            .clone()
            .get_by_name::<ScopedMultiNamedService>("second")
            .await
            .unwrap()
            .read()
            .await
            .value;

        assert_eq!(first_val, "first scoped");
        assert_eq!(second_val, "second scoped");
        assert_ne!(first_val, second_val);
    })
    .await;
}
