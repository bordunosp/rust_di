use crate::core::error_di::DiError;
use crate::core::factory::DiFactory;
use crate::{DIScope, initialize};
use std::sync::Arc;
use tokio::sync::Mutex;

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

struct ScopedNamedService {
    pub value: Mutex<&'static str>,
}

impl Default for ScopedNamedService {
    fn default() -> Self {
        ScopedNamedService {
            value: Mutex::new("default"),
        }
    }
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
            // Тепер мутуємо через замок всередині сервісу
            let mut guard = instance.value.lock().await;
            *guard = "updated named";
        }

        let instance = scope
            .clone()
            .get_by_name::<ScopedNamedService>("named")
            .await
            .unwrap();

        let final_val = *instance.value.lock().await;
        assert_eq!(final_val, "updated named");
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
        let value = instance.value;
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
        let value = instance.value;
        assert_eq!(value, "auto scoped");
    })
    .await;
}

struct ScopedIsolatedService {
    pub value: Mutex<&'static str>,
}

impl Default for ScopedIsolatedService {
    fn default() -> Self {
        ScopedIsolatedService {
            value: Mutex::new("default"),
        }
    }
}

#[rust_di::registry(Scoped)]
impl ScopedIsolatedService {}

#[tokio::test]
async fn test_scoped_returns_new_instance_for_each_scope() {
    initialize().await;

    // Перший скоуп
    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let instance = scope.get::<ScopedIsolatedService>().await.unwrap();
        *instance.value.lock().await = "scope a";
        // Тут ми не можемо винести Arc за межі scope (чере Drop DIScope),
        // тому просто збережемо значення
    })
    .await;

    // Другий скоуп
    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();
        let instance = scope.get::<ScopedIsolatedService>().await.unwrap();

        // Перевіряємо, що новий скоуп має дефолтне значення, а не "scope a"
        let current_val = *instance.value.lock().await;
        assert_eq!(current_val, "default");
    })
    .await;
}

struct ScopedMultiNamedService {
    pub value: Mutex<&'static str>,
}

impl Default for ScopedMultiNamedService {
    fn default() -> Self {
        ScopedMultiNamedService {
            value: Mutex::new("default"),
        }
    }
}

#[rust_di::registry(Scoped(name = "first"))]
#[rust_di::registry(Scoped(name = "second"))]
impl ScopedMultiNamedService {}

#[tokio::test]
async fn test_scoped_multiple_named_instances_are_distinct() {
    initialize().await;
    DIScope::run_with_scope(|| async {
        let scope = DIScope::current().unwrap();

        let first = scope
            .clone()
            .get_by_name::<ScopedMultiNamedService>("first")
            .await
            .unwrap();
        *first.value.lock().await = "first scoped";

        let second = scope
            .clone()
            .get_by_name::<ScopedMultiNamedService>("second")
            .await
            .unwrap();
        *second.value.lock().await = "second scoped";

        assert_eq!(*first.value.lock().await, "first scoped");
        assert_eq!(*second.value.lock().await, "second scoped");
        assert!(!Arc::ptr_eq(&first, &second));
    })
    .await;
}
