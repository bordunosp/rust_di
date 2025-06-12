use di::DIScope;
use with_di_scope::with_di_scope;

#[with_di_scope]
async fn example_function() {
    println!("Inside example function");
    let _ = DIScope::current().unwrap();
}

fn main() {
    // This is a UI test, so it's not actually run, just compiled.
    // If it compiles, the test passes.
    // Tokio runtime is not required here for `trybuild` to compile it.
}
