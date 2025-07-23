#[derive(Debug)]
pub struct DiConstructor {
    pub init: fn() -> std::pin::Pin<Box<dyn Future<Output = ()> + Send>>,
}

inventory::collect!(DiConstructor);
