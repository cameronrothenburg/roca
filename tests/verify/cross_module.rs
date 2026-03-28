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
        pub struct Email {
            value: String
            create(raw: String) -> Email
        }{
            fn create(raw: String) -> Email {
                return Email { value: raw }
                test {}
            }
        }

        Email satisfies String {
            fn trim() -> String {
                return self.value.trim()
                crash { self.value.trim -> halt }
                test {}
            }
            fn toString() -> String {
                return self.value
                test {}
            }
            fn includes(search: String) -> Bool {
                return self.value.includes(search)
                crash { self.value.includes -> halt }
                test {}
            }
        }
    "#).unwrap();

    fs::write(dir.join("user.roca"), r#"
        import { Email } from "./email.roca"

        pub fn process_email(raw: String) -> String {
            const email = Email.create(raw)
            const trimmed = email.trim()
            return trimmed
            crash {
                Email.create -> halt
                email.trim -> halt
            }
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
    let errors = roca::check::check_with_registry(&user_file, &project.registry);

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

        pub struct Secret {
            value: String
            create(v: String) -> Secret
        }{
            fn create(v: String) -> Secret {
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

        pub fn handle(raw: String) -> String {
            const s = Secret.create(raw)
            log(s.to_log())
            return "done"
            crash {
                Secret.create -> halt
                s.to_log -> halt
                log -> halt
            }
            test { self("password") == "done" }
        }
    "#).unwrap();

    let project = roca::resolve::resolve_directory(Path::new(&dir));

    assert!(project.registry.type_satisfies("Secret", "Loggable"),
        "Secret should satisfy Loggable across modules");

    let app_source = fs::read_to_string(dir.join("app.roca")).unwrap();
    let app_file = roca::parse::parse(&app_source);
    let errors = roca::check::check_with_registry(&app_file, &project.registry);

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
        pub struct Email {
            value: String
            create(raw: String) -> Email
        }{
            fn create(raw: String) -> Email {
                return Email { value: raw }
                test {}
            }
        }

        Email satisfies String {
            fn trim() -> String {
                return self.value.trim()
                crash { self.value.trim -> halt }
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

    // Write a runner that uses Email.trim() from JS
    fs::write(dir.join("run.js"), r#"
        import { Email } from "./email.js";
        const e = Email.create(" cam@test.com ");
        console.log(e.trim());
    "#).unwrap();

    let output = std::process::Command::new("bun")
        .arg(dir.join("run.js").to_str().unwrap())
        .output()
        .expect("failed to run bun");

    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let _ = fs::remove_dir_all(&dir);

    assert_eq!(stdout, "cam@test.com",
        "Email.trim() should work cross-module, got: {}", stdout);
}
