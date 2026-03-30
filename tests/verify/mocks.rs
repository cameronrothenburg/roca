use super::harness::run_with_tests;

#[test]
fn mock_object_emitted_for_contract() {
    // Need at least one function with a test block for emit_tests to produce output
    let file = roca::parse::parse(r#"
        contract Database {
            save(data: String) -> Ok, err {
                err timeout = "timed out"
            }
            mock {
                save -> Ok
            }
        }
        pub fn dummy() -> String {
            return "ok"
            test { self() == "ok" }
        }
    "#);
    let result = roca::emit::test_harness::emit_tests(&file, "__embed__");
    assert!(result.is_some());
    let (js, _) = result.unwrap();
    assert!(js.contains("__mock_Database"), "should emit mock object, got:\n{}", js);
    assert!(js.contains("save"), "mock should have save method");
}

#[test]
fn mock_with_string_return() {
    let file = roca::parse::parse(r#"
        contract FileSystem {
            read(path: String) -> String, err {
                err not_found = "not found"
            }
            mock {
                read -> "mock file content"
            }
        }

        pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }
    "#);
    let result = roca::emit::test_harness::emit_tests(&file, "__embed__");
    let (js, _) = result.unwrap();
    assert!(js.contains("__mock_FileSystem"), "should emit FileSystem mock");
    assert!(js.contains("mock file content"), "mock should return the string");
}

#[test]
fn mock_callable_in_test_harness() {
    // Verify the mock object is actually callable JS
    assert_eq!(run_with_tests(
        r#"
        contract Cache {
            get(key: String) -> String, err {
                err miss = "cache miss"
            }
            mock {
                get -> "cached_value"
            }
        }

        pub fn lookup() -> String {
            return "ok"
            test { self() == "ok" }
        }
        "#,
        r#"
            // The mock should be available as __mock_Cache
            console.log(typeof __mock_Cache);
            console.log(__mock_Cache.get("key"));
        "#,
    ), "1 passed, 0 failed\nobject\ncached_value");
}

#[test]
fn mock_ok_return() {
    assert_eq!(run_with_tests(
        r#"
        contract Database {
            save(data: String) -> Ok, err {
                err failed = "save failed"
            }
            mock {
                save -> Ok
            }
        }

        pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }
        "#,
        r#"
            const result = __mock_Database.save("test");
            console.log(result);
        "#,
    ), "1 passed, 0 failed\nnull");
}

#[test]
fn multiple_mock_methods() {
    assert_eq!(run_with_tests(
        r#"
        contract HttpClient {
            get(url: String) -> String, err {
                err timeout = "timed out"
            }
            post(url: String, body: String) -> String, err {
                err timeout = "timed out"
            }
            mock {
                get -> "get_response"
                post -> "post_response"
            }
        }

        pub fn test_fn() -> String {
            return "ok"
            test { self() == "ok" }
        }
        "#,
        r#"
            console.log(__mock_HttpClient.get("url"));
            console.log(__mock_HttpClient.post("url", "body"));
        "#,
    ), "1 passed, 0 failed\nget_response\npost_response");
}
