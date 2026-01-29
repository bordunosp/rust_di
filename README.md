[![Crates.io](https://img.shields.io/crates/v/rust_di.svg)](https://crates.io/crates/rust_di)
![Build Status](https://github.com/bordunosp/rust_di/actions/workflows/rust.yml/badge.svg)
[![Docs.rs](https://docs.rs/rust_di/badge.svg)](https://docs.rs/rust_di)
[![License](https://img.shields.io/crates/l/rust_di)](https://crates.io/crates/rust_di)
[![Downloads](https://img.shields.io/crates/d/rust_di.svg?style=flat-square)](https://crates.io/crates/rust_di)

# 🧩 `rust_di` — Declarative, Async-Safe Dependency Injection for Rust

---

## ✨ Highlights

* 🚀 Async-first architecture (factory-based, scoped resolution)
* 🧠 Lifetimes: Singleton, Scoped, Transient
* 📛 Named service instances
* 💡 Declarative registration via #[rust_di::registry(...)]
* 🔁 Task-local isolation (tokio::task_local!)
* 🧰 Procedural macros with zero boilerplate
* 🧪 Circular dependency detection
* 📦 Thread-safe (using Arc, RwLock, DashMap, ArcSwap, OnceCell)

---

## ⚡️ Getting Started

### 1. Add to `Cargo.toml`

```toml
[dependencies]
rust_di = { version = "3.1.1" }
```

### 2. Register Services (in a way convenient for you)

```rust
#[derive(Default)]
pub struct Logger;

#[rust_di::registry(
    Singleton,
    Singleton(factory),
    Singleton(name = "file_logger"),
    Singleton(name = "console_logger"),
    Singleton(name = "email_logger", factory = EmailLoggerFactory),

    Transient,
    Transient(factory),
    Transient(name = "file_logger"),
    Transient(name = "console_logger"),
    Transient(name = "email_logger", factory = EmailLoggerFactory),

    Scoped,
    Scoped(factory),
    Scoped(name = "file_logger"),
    Scoped(name = "console_logger"),
    Scoped(name = "email_logger", factory = EmailLoggerFactory),
)]
impl Logger {
    pub fn log(&self, msg: &str) { 
        println!("{}", msg); 
    }
}

```

---

### 3. Resolve Inside Scope

### 🧮 Scope Bootstrapping

Before resolving any services, make sure to initialize the DI system:

```rust
#[tokio::main]
async fn main() {
    rust_di::initialize().await;
}
```

#### This sets up:

* All services declared via inventory::submit!
* Global singletons & factories
* Internal caches and resolving state

#### You only need to call it once, typically at the beginning of main() or your test setup.

-----

### 🔍 Example: Main Function with Initialization

```rust
#[tokio::main]
async fn main() {
    rust_di::initialize().await;

    rust_di::DIScope::run_with_scope(|| async {
        let di = rust_di::DIScope::current().unwrap();

        let logger = di.clone().get::<Logger>().await.unwrap();
        logger.log("Hello!");

        let file_logger = di.get_by_name::<Logger>("file").await.unwrap();
        file_logger.log("Writing to file...");
    }).await;
}
```

# 🧠 Async Entrypoint — `#[rust_di::main]`

Use `#[rust_di::main]` to simplify your async `fn main`. It ensures:

* ✅ rust_di::initialize().await
* ✅ DIScope::run_with_scope(...)
* ✅ DI services available from the start


### 🧪 Example

```rust
#[rust_di::main]
#[tokio::main]
async fn main() {
    let scope = rust_di::DIScope::current().unwrap();
    let logger = scope.get::<Logger>().await.unwrap();
    logger.log("Started!");
}
```

### ⚠️ Must be placed above #[tokio::main] to work correctly.

---

## 🌀 Automatic DI Scope Initialization - `#[with_di_scope]`

#### ⚠️ The `#[rust_di::with_di_scope]` macro works only on standalone

`async fn`, not on trait methods or functions wrapped with conflicting attribute macros such as `#[tokio::main]` or
`#[test]`.

#### ✅ Use it for plain

`async fn` entrypoints, background workers, or utility functions where full DI context is needed.

```rust
#[rust_di::with_di_scope]
async fn consume_queue() {
    let di = DIScope::current().unwrap();
    let consumer = di.get::<Consumer>().await.unwrap();
    consumer.run().await;
}
```

#### 🧠 This macro fully replaces the manual block shown in section 3. Resolve services.

This pattern is ideal for long-running background tasks, workers, or event handlers that need access to scoped services.

---

### ✅ Why use `#[with_di_scope]`?

* Eliminates boilerplate around `DIScope::run_with_scope`
* Ensures `task-local` variables are properly initialized
* Works seamlessly in `main`, `background loops`, or any `async entrypoint`
* Encourages `clean`, scoped service resolution

---

## 🔄 Service Dependencies via `DiFactory`

#### You can declare service dependencies by implementing `DiFactory`. 
#### This allows a service to resolve other services during its creation:

```rust
use rust_di::DIScope;
use rust_di::core::error_di::DiError;
use rust_di::core::factory::DiFactory;
use rust_di::registry;
use std::sync::Arc;

#[derive(Default)]
pub struct Logger;

#[registry(Singleton)]
impl Logger {}

pub struct Processor {
    pub logger: Arc<Logger>,
}

#[registry(Singleton(factory))]
impl Processor {}

#[async_trait::async_trait]
impl DiFactory for Processor {
    async fn create(scope: Arc<DIScope>) -> Result<Self, DiError> {
        let logger = scope.get::<Logger>().await?;
        Ok(Processor {
            logger: logger.clone(),
        })
    }
}
```

#### The `DiFactory` is automatically invoked if factory is enabled in #[registry(...)].

### ✨ Factory Benefits

* 🔧 Resolves dependencies with async precision
* 🎯 Keeps instantiation logic colocated
* 🧩 Enables complex composition across lifetimes

---

## ✋ Manual Service Registration

In some situations—like ordering guarantees, test injection, or dynamic setup—you may want to bypass macros and register
manually:

```rust
use rust_di::DIScope;
use rust_di::core::error_di::DiError;
use rust_di::core::registry::register_singleton_name;

#[derive(Default)]
pub struct Logger;

#[tokio::main]
async fn main() -> Result<(), DiError> {
    rust_di::initialize().await;

    // Manual registration
    register_singleton_name::<Logger, _, _>("file", |_| async { Ok(Logger::default()) }).await?;

    DIScope::run_with_scope(|| async {
        let di = DIScope::current().unwrap();
        let logger = di.get_by_name::<Logger>("file").await?;
        logger.log("Manual registration works!");
        Ok(())
    }).await
}
```

---

## 🧠 Manual API Available

Function Description
register_singleton unnamed global instance
register_singleton_name(name)    named global instance
register_scope_name(name)    scoped factory
register_transient_name(name)    re-created per request

| Function                | Description                  |
|:------------------------|:-----------------------------|
| register_transient      | re-created per request       |
| register_transient_name | named re-created per request |
| register_scope          | scoped factory               |
| register_scope_name     | named scoped factory         |
| register_singleton      | unnamed global instance      |
| register_singleton_name | named global instance        |

#### All support factories and return Result.

#### 📚 These extensions give you full control—whether bootstrapping large systems, injecting mocks in tests, or dynamically assembling modules.

---

## 🔐 Safety Model

* Services stored as `Arc<T>`
* Global state managed via `OnceCell` & `ArcSwap`
* Scope-local cache via `DashMap`
* Panics on usage outside active DI scope
* Circular dependency errors on recursive resolutions

---

### 🧠 Lifetimes

| Lifetime  | Behavior                                                    |
|:----------|:------------------------------------------------------------|
| Singleton | One instance per App.<br/> Global, shared across all scopes |
| Scoped    | Created one instance per DIScope::run_with_scope()          |
| Transient | New instance every time<br/>Re-created on every .get()      |

### 🧰 Procedural Macro

Supports:

* Singleton, Scoped, Transient
* factory — use `DiFactory` or `custom factory`
* name = "..." — register named instance

---

### 🔒 Safety

* All services are stored as `Arc<T>`
* Internally uses `DashMap`, `ArcSwap`, and `OnceCell`
* `Task-local` isolation via `tokio::task_local!`

---

### ⚠️ Limitation: `tokio::spawn` drops DI context

Because `DIScope` relies on `task-local` variables (`tokio::task_local!`), spawning a new task with `tokio::spawn` will
lose the current DI scope context.

```rust
tokio::spawn( async {
    // ❌ This will panic: no DI scope found
    let scope = DIScope::current().unwrap();
});
```

### ✅ Workaround

If you need to spawn a task that uses DI, wrap the task in a new scope:

```rust
tokio::spawn( async {
    rust_di::DIScope::run_with_scope(|| async {
        let scope = di::DIScope::current().unwrap();
        let logger = scope.get::< Logger > ().await.unwrap();
        logger.log("Inside spawned task");
    }).await;
});
```

Alternatively, pass the resolved dependencies into the task before spawning.


--- 

# #StandForUkraine 🇺🇦

This project aims to show support for Ukraine and its people amidst a war that has been ongoing since 2014. This war has
a genocidal nature and has led to the deaths of thousands, injuries to millions, and significant property damage. We
believe that the international community should focus on supporting Ukraine and ensuring security and freedom for its
people.

Join us and show your support using the hashtag #StandForUkraine. Together, we can help bring attention to the issues
faced by Ukraine and provide aid.


