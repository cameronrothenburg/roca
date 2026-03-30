use super::harness::run;

#[test]
fn contract_errors_object() {
    assert_eq!(run(
        r#"contract HttpClient {
            get(url: String) -> String, err {
                err timeout = "request timed out"
                err not_found = "404 not found"
            }
        }"#,
        r#"
            console.log(HttpClientErrors.timeout);
            console.log(HttpClientErrors.not_found);
        "#,
    ), "request timed out\n404 not found");
}

#[test]
fn contract_multiple_methods_errors() {
    assert_eq!(run(
        r#"contract Database {
            save(data: String) -> String, err {
                err connection = "connection lost"
                err duplicate = "duplicate key"
            }
            find(id: String) -> String, err {
                err not_found = "not found"
            }
        }"#,
        r#"
            console.log(DatabaseErrors.connection);
            console.log(DatabaseErrors.duplicate);
            console.log(DatabaseErrors.not_found);
        "#,
    ), "connection lost\nduplicate key\nnot found");
}

#[test]
fn enum_contract_values() {
    assert_eq!(run(
        r#"contract StatusCode { 200 201 400 404 500 }"#,
        r#"
            console.log(StatusCode["200"]);
            console.log(StatusCode["404"]);
            console.log(StatusCode["500"]);
        "#,
    ), "200\n404\n500");
}

#[test]
fn contract_no_errors_no_object() {
    // A contract with no errors should not emit an Errors object
    assert_eq!(run(
        r#"contract Stringable { to_string() -> String }"#,
        r#"
            console.log(typeof StringableErrors);
        "#,
    ), "undefined");
}
