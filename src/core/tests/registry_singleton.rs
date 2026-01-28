use crate::initialize;
use rust_di::DIScope;
use rust_di::DiError;
use rust_di::core::factory::DiFactory;
use std::sync::Arc;
use tokio::sync::Mutex;

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
        let value = instance.value;
        assert_eq!(value, "default");
    })
    .await;
}

struct NamedService {
    pub value: Mutex<&'static str>,
}

impl Default for NamedService {
    fn default() -> Self {
        NamedService {
            value: Mutex::new("default"),
        }
    }
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
            let mut val_guard = instance.value.lock().await;
            *val_guard = "custom_name 22";
        }

        let instance = scope
            .get_by_name::<NamedService>("custom_name")
            .await
            .unwrap();
        let final_value = *instance.value.lock().await;
        assert_eq!(final_value, "custom_name 22");
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
        let value = instance.value;
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
        let value = instance.value;
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
        let value = instance.value;
        assert_eq!(value, "auto");
    })
    .await;
}

struct MultiNamedService {
    pub value: Mutex<&'static str>,
}

impl Default for MultiNamedService {
    fn default() -> Self {
        MultiNamedService {
            value: Mutex::new("default"),
        }
    }
}

#[rust_di::registry(Singleton(name = "first"))]
#[rust_di::registry(Singleton(name = "second"))]
impl MultiNamedService {}

#[tokio::test]
async fn test_multiple_named_instances_are_distinct() {
    initialize().await;

    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();

        // Отримуємо і змінюємо перший
        let first = scope
            .clone()
            .get_by_name::<MultiNamedService>("first")
            .await
            .unwrap();
        {
            let mut g = first.value.lock().await;
            *g = "updated first";
        }

        // Отримуємо і змінюємо другий
        let second = scope
            .clone()
            .get_by_name::<MultiNamedService>("second")
            .await
            .unwrap();
        {
            let mut g = second.value.lock().await;
            *g = "updated second";
        }

        let v1 = *first.value.lock().await;
        let v2 = *second.value.lock().await;

        assert_eq!(v1, "updated first");
        assert_eq!(v2, "updated second");
        assert_ne!(v1, v2);

        // Перевірка, що це різні об'єкти в пам'яті
        assert!(!Arc::ptr_eq(&first, &second));
    })
    .await;
}
