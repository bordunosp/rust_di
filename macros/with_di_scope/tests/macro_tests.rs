use with_di_scope::with_di_scope;

#[test]
fn ui_tests() {
    let t = trybuild::TestCases::new();
    let ui_tests_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("ui");

    t.compile_fail(ui_tests_path.join("non_async_fn.rs"));
    t.pass(ui_tests_path.join("basic_usage.rs"));
    t.pass(ui_tests_path.join("access_scope.rs"));
}

#[tokio::test]
async fn integration_test_with_scope() {
    use di::test_utils::reset_global_di_state_for_tests;
    use di::{DIScope, register_transient};

    #[with_di_scope]
    async fn my_test_function_inner() -> Result<usize, di::DiError> {
        register_transient(|_| async move { Ok(42usize) }).await?;

        let scope = DIScope::current()?;
        let service = scope.get::<usize>().await?;
        Ok(*service.read().await)
    }

    reset_global_di_state_for_tests().await.unwrap();
    let result = my_test_function_inner().await.unwrap();
    assert_eq!(result, 42);

    reset_global_di_state_for_tests().await.unwrap();
    let second_result = my_test_function_inner().await.unwrap();
    assert_eq!(second_result, 42);
}
