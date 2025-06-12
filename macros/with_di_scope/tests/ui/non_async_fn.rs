use with_di_scope::with_di_scope;

#[with_di_scope]
fn sync_function() {
    println!("This should not compile");
}

fn main() {}
