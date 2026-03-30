
#[test]
fn import_parsed_and_emitted() {
    // Verify import statement parses and emits correct JS
    let file = roca::parse::parse(r#"
        import { Email, User } from "./types.roca"

        pub fn greet(name: String) -> String {
            return "Hello " + name
            test { self("cam") == "Hello cam" }
        }
    "#);

    let js = roca::emit::emit(&file);
    assert!(js.contains("import { Email, User } from \"./types.js\""));
    assert!(js.contains("function greet"));
}

#[test]
fn import_rewrites_roca_to_js() {
    let file = roca::parse::parse(r#"
        import { Config } from "./config.roca"
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("./config.js"));
    assert!(!js.contains(".roca"));
}

#[test]
fn multiple_imports() {
    let file = roca::parse::parse(r#"
        import { Email } from "./email.roca"
        import { User } from "./user.roca"

        pub fn check() -> String {
            return "ok"
            test { self() == "ok" }
        }
    "#);
    let js = roca::emit::emit(&file);
    assert!(js.contains("from \"./email.js\""));
    assert!(js.contains("from \"./user.js\""));
}

#[test]
fn cross_file_execution() {
    // Write two JS files, verify they work together via bun
    use std::fs;

    let types_roca = r#"
        pub struct Email {
            value: String
            validate(raw: String) -> Email, err {
                err invalid = "bad email"
            }
        }{
            fn validate(raw: String) -> Email, err {
                if raw == "" { return err.invalid }
                return Email { value: raw }
                test {
                    self("a@b.com") is Ok
                    self("") is err.invalid
                }
            }
        }
    "#;

    let main_roca = r#"
        import { Email } from "./types.js"

        pub fn create_email(raw: String) -> String, err {
            let result, e = Email.validate(raw)
            if e { return err.failed }
            return result.value
            crash { Email.validate -> halt }
            test {
                self("a@b.com") == "a@b.com"
            }
        }
    "#;

    // Emit both
    let types_file = roca::parse::parse(types_roca);
    let types_js = roca::emit::emit(&types_file);
    let main_file = roca::parse::parse(main_roca);
    let main_js = roca::emit::emit(&main_file);

    // Write to temp files
    let dir = std::env::temp_dir().join("roca_test_imports");
    let _ = fs::create_dir_all(&dir);
    fs::write(dir.join("types.js"), &types_js).unwrap();
    fs::write(dir.join("main.js"), &main_js).unwrap();

    // Inline both modules + test code (strip exports/imports)
    let types_inline = types_js.replace("export ", "");
    let main_inline = main_js.replace("export ", "")
        .lines().filter(|l| !l.starts_with("import ")).collect::<Vec<_>>().join("\n");
    let test_code = format!(
        "{}\n{}\nconst {{ value: v, err: e }} = create_email(\"cam@test.com\");\nconsole.log(v);\nconsole.log(e);",
        types_inline, main_inline
    );
    let (stdout, _) = roca::cli::runtime::run_tests(&test_code);
    let stdout = stdout.trim().to_string();

    // Clean up
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(stdout, "cam@test.com\nnull", "cross-file import failed: {}", stdout);
}
