use std::fs;
use std::path::Path;

#[test]
fn cross_module_satisfies_resolved() {
    // email.roca defines Email satisfies String
    // user.roca imports Email, calls .trim() on it — should work because String is satisfied
    let dir = std::env::temp_dir().join("roca_cross_module_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("email.roca"), r#"
        /// An email address
        pub struct Email {
            value: String
            create(raw: String) -> Email
        }{
            pub fn create(raw: String) -> Email {
                return Email { value: raw }
                test {}
            }
        }

        Email satisfies String {
            fn trim() -> String {
                return self.value.trim()
                crash { self.value.trim -> skip }
                test {}
            }
            fn toString() -> String {
                return self.value
                test {}
            }
            fn includes(search: String) -> Bool {
                return self.value.includes(search)
                crash { self.value.includes -> skip }
                test {}
            }
        }
    "#).unwrap();

    fs::write(dir.join("user.roca"), r#"
        import { Email } from "./email.roca"

        /// Processes an email
        pub fn process_email(raw: String) -> String {
            const email = Email.create(raw)
            const trimmed = email.trim()
            return trimmed
            test { self(" cam@test.com ") == "cam@test.com" }
        }
    "#).unwrap();

    // Resolve the project
    let project = roca::resolve::resolve_directory(Path::new(&dir));

    // Check that Email satisfies String is visible across modules
    assert!(project.registry.type_satisfies("Email", "String"),
        "Email should satisfy String in cross-module resolution");

    // Check user.roca against the shared registry — should pass
    let user_source = fs::read_to_string(dir.join("user.roca")).unwrap();
    let user_file = roca::parse::parse(&user_source);
    let errors = roca::check::check_with_registry_and_dir(&user_file, &project.registry, Some(&dir));

    let _ = fs::remove_dir_all(&dir);

    assert!(errors.is_empty(),
        "expected no errors for cross-module satisfies, got: {:?}", errors);
}

#[test]
fn cross_module_loggable_resolved() {
    let dir = std::env::temp_dir().join("roca_cross_loggable_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    fs::write(dir.join("secret.roca"), r#"
        contract Loggable { to_log() -> String }

        /// A secret value that redacts on log
        pub struct Secret {
            value: String
            create(v: String) -> Secret
        }{
            pub fn create(v: String) -> Secret {
                return Secret { value: v }
                test {}
            }
        }

        Secret satisfies Loggable {
            fn to_log() -> String {
                return "REDACTED"
                test {}
            }
        }
    "#).unwrap();

    fs::write(dir.join("app.roca"), r#"
        import { Secret } from "./secret.roca"

        /// Handles a raw secret string
        pub fn handle(raw: String) -> String {
            const s = Secret.create(raw)
            log(s.to_log())
            return "done"
            test { self("password") == "done" }
        }
    "#).unwrap();

    let project = roca::resolve::resolve_directory(Path::new(&dir));

    assert!(project.registry.type_satisfies("Secret", "Loggable"),
        "Secret should satisfy Loggable across modules");

    let app_source = fs::read_to_string(dir.join("app.roca")).unwrap();
    let app_file = roca::parse::parse(&app_source);
    let errors = roca::check::check_with_registry_and_dir(&app_file, &project.registry, Some(&dir));

    let _ = fs::remove_dir_all(&dir);

    assert!(errors.is_empty(),
        "expected no errors, got: {:?}", errors);
}

#[test]
fn cross_module_method_on_imported_type() {
    // Verify that methods from satisfies blocks on imported types are available
    let dir = std::env::temp_dir().join("roca_cross_method_test");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();

    // email.roca — Email with trim via satisfies String
    fs::write(dir.join("email.roca"), r#"
        /// An email address
        pub struct Email {
            value: String
            create(raw: String) -> Email
        }{
            pub fn create(raw: String) -> Email {
                return Email { value: raw }
                test {}
            }
        }

        Email satisfies String {
            fn trim() -> String {
                return self.value.trim()
                crash { self.value.trim -> skip }
                test {}
            }
        }
    "#).unwrap();

    // Build both files, emit JS, run cross-module
    let project = roca::resolve::resolve_directory(Path::new(&dir));

    let email_source = fs::read_to_string(dir.join("email.roca")).unwrap();
    let email_file = roca::parse::parse(&email_source);
    let email_js = roca::emit::emit(&email_file);
    fs::write(dir.join("email.js"), &email_js).unwrap();

    // Run inline: inline email.js + test code (strip exports/imports)
    let inline_js = email_js.replace("export ", "");
    let test_code = format!("{}\nconst e = Email.create(\" cam@test.com \");\nconsole.log(e.trim());", inline_js);
    let (stdout, _) = roca::cli::runtime::run_tests(&test_code);
    let stdout = stdout.trim().to_string();
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(stdout, "cam@test.com",
        "Email.trim() should work cross-module, got: {}", stdout);
}

// ─── Cross-file crash-on-safe ──────────────────────────

#[test]
fn cross_file_crash_on_safe() {
    let dir = std::env::temp_dir().join("roca_cross_crash_safe");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(dir.join("src").join("helper.roca"), r#"
        /// A safe function that does not return errors
        pub fn safe_fn(s: String) -> String {
            return s
            test { self("a") == "a" }
        }
    "#).unwrap();

    fs::write(dir.join("src").join("main.roca"), r#"
        import { safe_fn } from "./helper.roca"

        /// Calls safe_fn with unnecessary crash block
        pub fn caller() -> String {
            const r = safe_fn("x")
            return r
            crash { safe_fn -> halt }
            test { self() == "x" }
        }
    "#).unwrap();

    let src_dir = dir.join("src");
    let project = roca::resolve::resolve_directory(&src_dir);
    let source = fs::read_to_string(src_dir.join("main.roca")).unwrap();
    let file = roca::parse::parse(&source);
    let errors = roca::check::check_with_registry_and_dir(&file, &project.registry, Some(&src_dir));
    let _ = fs::remove_dir_all(&dir);

    assert!(errors.iter().any(|e| e.code == "crash-on-safe"),
        "expected crash-on-safe for imported non-error function, got: {:?}", errors);
}

// ─── Cross-file arg type mismatch ──────────────────────

#[test]
fn cross_file_arg_type_mismatch() {
    let dir = std::env::temp_dir().join("roca_cross_arg_type");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(dir.join("src").join("math.roca"), r#"
        /// Adds two numbers
        pub fn add(a: Number, b: Number) -> Number {
            return a + b
            test { self(1, 2) == 3 }
        }
    "#).unwrap();

    fs::write(dir.join("src").join("main.roca"), r#"
        import { add } from "./math.roca"

        /// Calls add with wrong types
        pub fn bad() -> Number {
            return add("a", "b")
            test { self() == 0 }
        }
    "#).unwrap();

    let src_dir = dir.join("src");
    let project = roca::resolve::resolve_directory(&src_dir);
    let source = fs::read_to_string(src_dir.join("main.roca")).unwrap();
    let file = roca::parse::parse(&source);
    let errors = roca::check::check_with_registry_and_dir(&file, &project.registry, Some(&src_dir));
    let _ = fs::remove_dir_all(&dir);

    assert!(errors.iter().any(|e| e.code == "arg-type-mismatch"),
        "expected arg-type-mismatch for imported function, got: {:?}", errors);
}

// ─── Cross-file valid call passes ──────────────────────

#[test]
fn cross_file_valid_call_passes() {
    let dir = std::env::temp_dir().join("roca_cross_valid");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(dir.join("src").join("helper.roca"), r#"
        /// Greets by name
        pub fn greet(name: String) -> String {
            return "Hello " + name
            test { self("cam") == "Hello cam" }
        }
    "#).unwrap();

    fs::write(dir.join("src").join("main.roca"), r#"
        import { greet } from "./helper.roca"

        /// Uses greet correctly
        pub fn welcome(name: String) -> String {
            return greet(name)
            test { self("cam") == "Hello cam" }
        }
    "#).unwrap();

    let src_dir = dir.join("src");
    let project = roca::resolve::resolve_directory(&src_dir);
    let source = fs::read_to_string(src_dir.join("main.roca")).unwrap();
    let file = roca::parse::parse(&source);
    let errors = roca::check::check_with_registry_and_dir(&file, &project.registry, Some(&src_dir));
    let _ = fs::remove_dir_all(&dir);

    assert!(errors.is_empty(),
        "valid cross-file call should pass, got: {:?}", errors);
}

// ─── Cross-file unhandled error propagation ───────────

#[test]
fn cross_file_unhandled_error() {
    let dir = std::env::temp_dir().join("roca_cross_unhandled");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("src")).unwrap();

    fs::write(dir.join("src").join("db.roca"), r#"
        /// Fetches a record
        pub fn fetch(id: String) -> String, err {
            err not_found = "not found"
            if id == "" { return err.not_found }
            return "data"
            test { self("1") == "data" self("") is err.not_found }
        }
    "#).unwrap();

    // Caller halts on fetch but does NOT declare its own errors —
    // the unhandled-error rule should fire.
    fs::write(dir.join("src").join("main.roca"), r#"
        import { fetch } from "./db.roca"

        /// Uses fetch but does not declare errors
        pub fn get_data(id: String) -> String {
            const r = fetch(id)
            return r
            crash { fetch -> halt }
            test { self("1") == "data" }
        }
    "#).unwrap();

    let src_dir = dir.join("src");
    let project = roca::resolve::resolve_directory(&src_dir);
    let source = fs::read_to_string(src_dir.join("main.roca")).unwrap();
    let file = roca::parse::parse(&source);
    let errors = roca::check::check_with_registry_and_dir(&file, &project.registry, Some(&src_dir));
    let _ = fs::remove_dir_all(&dir);

    assert!(errors.iter().any(|e| e.code == "unhandled-error"),
        "expected unhandled-error for imported function with halt, got: {:?}", errors);
}
