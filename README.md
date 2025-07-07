# 🧩 di — Dependency Injection for Rust

A lightweight, async-friendly, scoped dependency injection container for Rust

---

## ✨ Features

- ✅ Singleton / Scoped / Transient lifetimes
- ✅ Named service instances
- ✅ Async factory support
- ✅ Circular dependency detection
- ✅ Procedural macro for registration
- ✅ Task-local scope isolation
- ✅ Thread-safe with `Arc` + `RwLock`

---

## 🚀 Quick Start

### 1. Add to `Cargo.toml`

```toml
[dependencies]
di = { git = "https://github.com/bordunosp/rust_di.git" }
ctor = "0.4" # Required for automatic handler & pipeline registration
```

**Why `ctor`?**

`di` uses the `ctor` crate to automatically register services at startup. Without it, nothing will be
wired up.

---

### 2. Register services

```toml
#[derive(Default)]
pub struct Logger;

#[di::registry(
Singleton,
Singleton(name = "file_logger", factory = FileLoggerFactory),
Transient(factory),
Scoped
)]
impl Logger { }
```

### 3. Resolve services

```rust
#[tokio::main]
async fn main() {
    di::DIScope::run_with_scope(|| async {
        let scope = di::DIScope::current().unwrap();

        let logger = scope.get::<Logger>().await.unwrap();
        logger.read().await.log("Hello from DI!");

        let file_logger = scope.get_by_name::<Logger>("file_logger").await.unwrap();
        file_logger.read().await.log("To file!");
    }).await;
}
```

### 🌀 Automatic DI Scope Initialization - `#[di::with_di_scope]`

The #[di::with_di_scope] macro wraps an async fn in DIScope::run_with_scope(...), automatically initializing the task-local context required for resolving dependencies.

---
### ✅ Example: Replacing main

You can replace the entire `DIScope::run_with_scope` block in your main function with a simple `macro`:

```rust
use di::{with_di_scope, DIScope};

#[di::registry(Singleton)]
impl Logger {}

#[di::with_di_scope]
async fn main() {
    let scope = DIScope::current().unwrap();
    let logger = scope.get::<Logger>().await.unwrap();
    logger.read().await.log("Hello from DI!");
}
```

## 🧠 This macro fully replaces the manual block shown in section 3. Resolve services.

---

### 🔁 Example: Background queue consumer loop

```rust
use di::{with_di_scope, DIScope};
use tokio::sync::mpsc::{self, Receiver};

#[derive(Default)]
struct QueueConsumer {
    queue: Receiver<String>,
}

#[di::registry(Singleton(factory = QueueConsumerFactory))]
impl QueueConsumer {}

async fn QueueConsumerFactory(_: std::sync::Arc<DIScope>) -> Result<QueueConsumer, di::DiError> {
    let (tx, rx) = mpsc::channel(100);
    tokio::spawn(async move {
        let _ = tx.send("Hello from queue".into()).await;
    });
    Ok(QueueConsumer { queue: rx })
}

#[with_di_scope]
async fn run_consumer_loop() {
    let scope = DIScope::current().unwrap();
    let consumer = scope.get::<QueueConsumer>().await.unwrap();

    while let Some(msg) = consumer.read().await.queue.recv().await {
        println!("Received: {msg}");
    }
}
```

This pattern is ideal for long-running background tasks, workers, or event handlers that need access to scoped services.

---

### ✅ Why use #[with_di_scope]?
* Eliminates boilerplate around `DIScope::run_with_scope`
* Ensures `task-local` variables are properly initialized
* Works seamlessly in `main`, `background loops`, or any `async entrypoint`
* Encourages `clean`, scoped service resolution

---

### 🧠 Lifetimes

| Lifetime  | Behavior                                 |
|:----------|:-----------------------------------------|
| Singleton | One instance per app                     |
| Scoped    | One instance per DIScope::run_with_scope |
| Transient | New instance every time                  |


### 🧰 Procedural Macro

Use `#[di::registry(...)]` to register services declaratively:

```rust
#[di::registry(
    Singleton,
    Scoped(factory),
    Transient(name = "custom")
)]
impl MyService {}
```

Supports:

* Singleton, Scoped, Transient
* factory — use `DiFactory` or `custom factory`
* name = "..." — register named instance

---

### 🧪 Testing

```
cargo test-default
```

Covers:

* Singleton caching
* Scoped reuse
* Transient instantiation
* Named resolution
* Circular dependency detection

---

### 🔒 Safety

* All services are stored as `Arc<RwLock<T>>`
* Internally uses `DashMap`, `ArcSwap`, and `OnceCell`
* Task-local isolation via `tokio::task_local!`
---


### ⚠️ Limitation: `tokio::spawn` drops DI context

Because `DIScope` relies on `task-local` variables (`tokio::task_local!`), spawning a new task with `tokio::spawn` will lose the current DI scope context.

```rust
tokio::spawn(async {
    // ❌ This will panic: no DI scope found
    let scope = DIScope::current().unwrap();
});
```

### ✅ Workaround
If you need to spawn a task that uses DI, wrap the task in a new scope:

```rust
tokio::spawn(async {
    di::DIScope::run_with_scope(|| async {
        let scope = di::DIScope::current().unwrap();
        let logger = scope.get::<Logger>().await.unwrap();
        logger.read().await.log("Inside spawned task");
    }).await;
});
```
Alternatively, pass the resolved dependencies into the task before spawning.


