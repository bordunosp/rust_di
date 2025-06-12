use di::DIScope;
use with_di_scope::with_di_scope;

#[with_di_scope]
async fn function_accessing_scope() -> Result<(), di::DiError> {
    let _scope = DIScope::current()?;
    Ok(())
}

fn main() {}
