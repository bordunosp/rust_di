# Dependency Injection Library for Async Rust

This is a lightweight, thread-safe dependency injection (DI) library designed for asynchronous Rust applications. It
provides three fundamental lifetime management strategies for your services: **singleton**, **scoped**, and **transient**.

---

### Features

- **Three Service Lifetimes**:
    - **Singleton**: A single instance is created and shared across the entire application's lifetime.
    - **Scoped**: An instance is created once per asynchronous task or DI scope and reused within that scope.
    - **Transient**: A new instance is created every time it's requested.
- **Thread-Safe**: Built with `ArcSwap` and `DashMap` for efficient and safe concurrent access to service registries.
- **Async-Friendly**: Seamlessly integrates with `tokio` for asynchronous service creation and resolution.
- **Named Services Support**: Register and resolve multiple instances of the same service type using unique names.
- **Circular Dependency Detection**: Prevents infinite loops during service resolution by detecting and reporting
  circular dependencies.
- **Task-Local Scoping**: Utilizes `tokio::task_local!` for efficient scope management within asynchronous tasks.

---

### Why Use It?

In large and complex applications, managing object lifecycles and their intricate dependencies can quickly become a
daunting task. This library helps address these challenges by allowing you to:

- **Reduce Coupling**: Your components don't need to know *how* to create their dependencies. Instead, they simply
  declare what they need, and the DI container provides the necessary instances, leading to cleaner, more modular code.
- **Improve Testability**: Easily swap out real service implementations for mock or stub versions during testing,
  enabling isolated unit and integration tests.
- **Manage Service Lifecycles**: With support for Singleton, Scoped, and Transient lifetimes, you gain fine-grained
  control over how your services are instantiated and reused, optimizing resource utilization.
- **Simplify Complex Applications**: Provides a centralized registry for all your services, making your application's
  architecture more predictable and easier to navigate.

This library is an excellent choice for web services, background processing tasks, and any asynchronous Rust application
where efficient and robust dependency management is crucial.

---

## Installing

Add the following to the [dependencies] section of your `Cargo.toml`:

```toml
[dependencies]
di = { git = "https://github.com/bordunosp/rust_di.git", tag = "0.1.0" }
```

---

## Basic Usage

Let's illustrate the core functionalities with examples.

### 1. Registering Services

You can register services with different lifetimes:

```rust
use di::*; // Import necessary traits and functions

#[tokio::main]
async fn main() {
    // Define some simple services for demonstration
    struct MySingletonService;
    impl MySingletonService {
        fn new() -> Self {
            println!("Singleton created!");
            Self
        }
    }

    struct MyScopedService;
    impl MyScopedService {
        fn new() -> Self {
            println!("Scoped created!");
            Self
        }
    }

    struct MyTransientService;
    impl MyTransientService {
        fn new() -> Self {
            println!("Transient created!");
            Self
        }
    }

    // Singleton: One instance for the entire application lifetime.
    // Instances are created immediately upon registration.
    di::register_singleton(MySingletonService::new()).await.unwrap();

    // Scoped: An instance is created once per DIScope (async task/request).
    // The factory function is called when first resolved within a scope.
    di::register_scope(|_| async { Ok(MyScopedService::new()) }).await.unwrap();

    // Transient: A new instance is created every time it's resolved.
    // The factory function is called for each resolution request.
    di::register_transient(|_| async { Ok(MyTransientService::new()) }).await.unwrap();

    println!("Services registered.");
}
```

### 2. Resolving Services

Services are resolved within an asynchronous `DIScope`. For scoped and transient services, the `DIScope` ensures correct
instance management.

```rust
use di::*;
use tokio::sync::RwLock;
use std::sync::Arc;

// Assume MyUserService and MyDatabaseConnection are defined elsewhere and registered.
// For example:
struct MyDatabaseConnection {
    pub id: usize
}
impl MyDatabaseConnection { fn new() -> Self { Self { id: 1 } } } // Example
struct MyUserRepository {
    db: Arc<RwLock<MyDatabaseConnection>>
}
impl MyUserRepository { async fn new(db: Arc<RwLock<MyDatabaseConnection>>) -> Self { Self { db } } } // Example
struct MyUserService {
    user_repo: Arc<RwLock<MyUserRepository>>
}
impl MyUserService { async fn new(user_repo: Arc<RwLock<MyUserRepository>>) -> Self { Self { user_repo } } } // Example

#[tokio::main]
async fn main() -> Result<(), di::DiError> {
    // Register your services (similar to the previous example)
    di::register_scope(|_| async { Ok(MyDatabaseConnection::new()) }).await?;
    di::register_transient(|scope| async move {
        let db = scope.get::<MyDatabaseConnection>().await?;
        Ok(MyUserRepository::new(db).await)
    }).await?;
    di::register_transient(|scope| async move {
        let user_repo = scope.get::<MyUserRepository>().await?;
        Ok(MyUserService::new(user_repo).await)
    }).await?;

    // To resolve services, you must be within a DIScope.
    // `run_with_scope` creates a new scope for the async block.
    di::DIScope::run_with_scope(|| async {
        let resolver = di::DIScope::current()?; // Get the current scope resolver

        // Resolve MyUserService. Its dependencies (MyUserRepository, MyDatabaseConnection)
        // will be resolved automatically based on their registered lifetimes.
        let user_service = resolver.get::<MyUserService>().await?;

        println!("MyUserService resolved successfully.");
        // You can now use user_service. E.g., access its inner data with .read().await
        let db_id = user_service.read().await.user_repo.read().await.db.read().await.id;
        println!("Database ID from resolved service: {}", db_id);

        Ok(())
    }).await
}
```

### 3. Named Services

You can register and resolve multiple services of the same type using unique names. This is particularly useful for
configuration or specialized implementations.

```rust
use di::*;

#[tokio::main]
async fn main() -> Result<(), di::DiError> {
    #[derive(Debug)]
    struct ConfigService {
        connection_string: String,
    }
    impl ConfigService {
        fn new(conn_str: String) -> Self { ConfigService { connection_string: conn_str } }
    }

    // Register named singleton instances
    di::register_singleton_name("primary_db_config", ConfigService::new("mongodb://localhost:27017".to_string())).await?;
    di::register_singleton_name("secondary_db_config", ConfigService::new("postgres://user:pass@host:5432/db".to_string())).await?;

    di::DIScope::run_with_scope(|| async {
        let resolver = di::DIScope::current()?;

        let primary_config = resolver.by_name::<ConfigService>("primary_db_config").await?;
        let secondary_config = resolver.by_name::<ConfigService>("secondary_db_config").await?;

        println!("Primary DB Config: {}", primary_config.read().await.connection_string);
        println!("Secondary DB Config: {}", secondary_config.read().await.connection_string);

        // Attempt to resolve an unnamed ConfigService (which doesn't exist)
        let default_config_result = resolver.get::<ConfigService>().await;
        if let Err(di::DiError::ServiceNotFound(_, name)) = default_config_result {
            println!("Default ConfigService not found as expected (name: '{}')", name);
        }

        Ok(())
    }).await
}
```

### 4. Circular Dependencies

The library detects and prevents infinite loops caused by circular dependencies during resolution. For truly circular
dependencies (e.g., Service A needs B, and B needs A), you can use `tokio::sync::OnceCell` for lazy initialization to
break the immediate cycle.

```rust
use di::*;
use tokio::sync::{OnceCell, RwLock};
use std::sync::Arc;

#[derive(Debug)]
struct LazyServiceA {
    b_lazy: OnceCell<Arc<RwLock<LazyServiceB>>>,
    scope: Arc<di::DIScope>,
}

impl LazyServiceA {
    async fn new(scope: Arc<di::DIScope>) -> Result<Self, di::DiError> {
        Ok(LazyServiceA { b_lazy: OnceCell::new(), scope })
    }

    pub async fn get_b(&self) -> Result<Arc<RwLock<LazyServiceB>>, di::DiError> {
        self.b_lazy.get_or_try_init(|| async {
            self.scope.clone().get::<LazyServiceB>().await
        }).await.map(|svc_ref| svc_ref.clone())
    }
}

#[derive(Debug)]
struct LazyServiceB {
    a_lazy: OnceCell<Arc<RwLock<LazyServiceA>>>,
    scope: Arc<di::DIScope>,
}

impl LazyServiceB {
    async fn new(scope: Arc<di::DIScope>) -> Result<Self, di::DiError> {
        Ok(LazyServiceB { a_lazy: OnceCell::new(), scope })
    }

    pub async fn get_a(&self) -> Result<Arc<RwLock<LazyServiceA>>, di::DiError> {
        self.a_lazy.get_or_try_init(|| async {
            self.scope.clone().get::<LazyServiceA>().await
        }).await.map(|svc_ref| svc_ref.clone())
    }
}

#[tokio::main]
async fn main() -> Result<(), di::DiError> {
    di::register_transient(|scope| async move { LazyServiceA::new(scope).await }).await?;
    di::register_transient(|scope| async move { LazyServiceB::new(scope).await }).await?;

    di::DIScope::run_with_scope(|| async {
        let resolver = di::DIScope::current()?;

        println!("Trying to resolve LazyServiceA...");
        let svc_a = resolver.clone().get::<LazyServiceA>().await?;

        println!("LazyServiceA resolved. Now trying to access LazyServiceB from LazyServiceA...");
        let svc_b_from_a = svc_a.read().await.get_b().await?;

        println!("LazyServiceB from LazyServiceA resolved. Now trying to access LazyServiceA from LazyServiceB...");
        let _svc_a_from_b = svc_b_from_a.read().await.get_a().await?;

        println!("Successfully resolved lazy circular dependencies by breaking the immediate cycle.");
        Ok(())
    }).await
}
```

### 5. Error Handling with Factories

Service factories can return custom errors, which will be wrapped in `DiError::FactoryError`.

```rust
use di::*;
use std::{error::Error, fmt};
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Debug)]
struct MyCustomServiceError {
    message: String,
    code: u16,
}

impl MyCustomServiceError {
    fn new(code: u16, message: &str) -> Self {
        MyCustomServiceError { code, message: message.to_string() }
    }
}

impl fmt::Display for MyCustomServiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Service Error [{}]: {}", self.code, self.message)
    }
}

impl Error for MyCustomServiceError {}

#[derive(Debug)]
struct MyServiceWithCustomError {
    data: String,
}

impl MyServiceWithCustomError {
    async fn new(should_fail: bool) -> Result<Self, MyCustomServiceError> {
        if should_fail {
            Err(MyCustomServiceError::new(500, "Failed to connect to external resource"))
        } else {
            Ok(MyServiceWithCustomError { data: "Initialized successfully".to_string() })
        }
    }
}

static FALLIBLE_SERVICE_COUNTER: AtomicUsize = AtomicUsize::new(0);

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
FALLIBLE_SERVICE_COUNTER.store(0, Ordering::SeqCst);

    di::register_transient(|_scope| async move {
        let current_count = FALLIBLE_SERVICE_COUNTER.fetch_add(1, Ordering::SeqCst);
        // Fail on even counts (0, 2, 4...), succeed on odd counts (1, 3, 5...)
        let should_fail = current_count % 2 == 0;
        println!("Attempting to create MyServiceWithCustomError (count: {})...", current_count + 1);
        MyServiceWithCustomError::new(should_fail).await
            // Map your custom error into DiError::FactoryError
            .map_err(|e| di::DiError::FactoryError(Box::new(e)))
    })
    .await?;

    di::DIScope::run_with_scope(|| async {
        let resolver = di::DIScope::current()?;

        println!("Attempt 1: Getting MyServiceWithCustomError...");
        let result1 = resolver.clone().get::<MyServiceWithCustomError>().await;
        match result1 {
            Err(di::DiError::FactoryError(e)) => {
                println!("Attempt 1 failed with error: {}", e);
                assert!(e.to_string().contains("Failed to connect to external resource"));
            },
            _ => panic!("Expected FactoryError on first attempt, got: {:?}", result1),
        }

        println!("Attempt 2: Getting MyServiceWithCustomError...");
        let result2 = resolver.clone().get::<MyServiceWithCustomError>().await;
        match result2 {
            Ok(service) => {
                println!("Attempt 2 successful. Service data: {:?}", service.read().await.data);
                assert!(service.read().await.data.contains("Initialized successfully"));
            },
            _ => panic!("Expected success on second attempt, got: {:?}", result2),
        }

        println!("Attempt 3: Getting MyServiceWithCustomError...");
        let result3 = resolver.clone().get::<MyServiceWithCustomError>().await;
        match result3 {
            Err(di::DiError::FactoryError(e)) => {
                println!("Attempt 3 failed with error: {}", e);
                assert!(e.to_string().contains("Failed to connect to external resource"));
            },
            _ => panic!("Expected FactoryError on third attempt, got: {:?}", result3),
        }

        Ok(())
    }).await
}
