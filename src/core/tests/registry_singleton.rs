use crate::initialize;
use rust_di::DIScope;
use rust_di::DiError;
use rust_di::core::factory::DiFactory;
use std::sync::Arc;

struct SimpleService {
    pub value: &'static str,
}

impl Default for SimpleService {
    fn default() -> Self {
        SimpleService::new()
    }
}

#[rust_di::registry(Singleton)]
impl SimpleService {
    pub fn new() -> Self {
        SimpleService { value: "default" }
    }
}

#[tokio::test]
async fn test_singleton_default_registration() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let instance = scope.get::<SimpleService>().await.unwrap();
        let value = instance.read().await.value;
        assert_eq!(value, "default");
    })
    .await;
}

#[derive(Default)]
struct NamedService {
    pub value: &'static str,
}

#[rust_di::registry(Singleton(name = "custom_name"))]
impl NamedService {}

#[tokio::test]
async fn test_singleton_named_registration() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        {
            let instance = scope
                .clone()
                .get_by_name::<NamedService>("custom_name")
                .await
                .unwrap();
            let mut srv = instance.write().await;
            srv.value = "custom_name 22";
        }

        let instance = scope
            .get_by_name::<NamedService>("custom_name")
            .await
            .unwrap();
        let value = instance.read().await.value;
        assert_eq!(value, "custom_name 22");
    })
    .await;
}

struct FactoryService {
    pub value: &'static str,
}

#[async_trait::async_trait]
impl DiFactory for FactoryService {
    async fn create(_: Arc<DIScope>) -> Result<Self, DiError> {
        Ok(FactoryService { value: "factory" })
    }
}

#[rust_di::registry(Singleton(factory = FactoryService))]
impl FactoryService {}

#[tokio::test]
async fn test_singleton_factory_registration() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let instance = scope.get::<FactoryService>().await.unwrap();
        let value = instance.read().await.value;
        assert_eq!(value, "factory");
    })
    .await;
}

struct NamedFactoryService {
    pub value: &'static str,
}

#[async_trait::async_trait]
impl DiFactory for NamedFactoryService {
    async fn create(_: Arc<DIScope>) -> Result<Self, DiError> {
        Ok(NamedFactoryService {
            value: "named factory",
        })
    }
}

#[rust_di::registry(Singleton(factory = NamedFactoryService, name = "custom_factory"))]
impl NamedFactoryService {}

#[tokio::test]
async fn test_singleton_named_factory_registration() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let instance = scope
            .get_by_name::<NamedFactoryService>("custom_factory")
            .await
            .unwrap();
        let value = instance.read().await.value;
        assert_eq!(value, "named factory");
    })
    .await;
}

struct AutoFactoryService {
    pub value: &'static str,
}

#[async_trait::async_trait]
impl DiFactory for AutoFactoryService {
    async fn create(_: Arc<DIScope>) -> Result<Self, DiError> {
        Ok(AutoFactoryService { value: "auto" })
    }
}

#[rust_di::registry(Singleton(factory))]
impl AutoFactoryService {}

#[tokio::test]
async fn test_singleton_auto_factory_registration() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let instance = scope.get::<AutoFactoryService>().await.unwrap();
        let value = instance.read().await.value;
        assert_eq!(value, "auto");
    })
    .await;
}

#[derive(Default)]
struct MultiNamedService {
    pub value: &'static str,
}

#[rust_di::registry(Singleton(name = "first"))]
#[rust_di::registry(Singleton(name = "second"))]
impl MultiNamedService {}

#[tokio::test]
async fn test_multiple_named_instances_are_distinct() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();

        {
            // Змінюємо значення першого інстансу
            let instance = scope
                .clone()
                .get_by_name::<MultiNamedService>("first")
                .await
                .unwrap();
            let mut srv = instance.write().await;
            srv.value = "updated first";
        }

        {
            // Змінюємо значення другого інстансу
            let instance = scope
                .clone()
                .get_by_name::<MultiNamedService>("second")
                .await
                .unwrap();
            let mut srv = instance.write().await;
            srv.value = "updated second";
        }

        let first_value = scope
            .clone()
            .get_by_name::<MultiNamedService>("first")
            .await
            .unwrap()
            .read()
            .await
            .value;

        let second_value = scope
            .clone()
            .get_by_name::<MultiNamedService>("second")
            .await
            .unwrap()
            .read()
            .await
            .value;

        assert_eq!(first_value, "updated first");
        assert_eq!(second_value, "updated second");
        assert_ne!(first_value, second_value);
    })
    .await;
}
