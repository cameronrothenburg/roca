use super::harness::run;

#[test]
fn adapter_pattern_basic() {
    // Extern contract as struct field, method called through field chain
    assert_eq!(run(
        r#"
        extern contract Database {
            query(sql: String) -> String, err {
                err failed = "query failed"
            }
        }

        pub struct Runtime {
            db: Database
        }{}

        pub fn get_user(rt: Runtime) -> String {
            let result, err = rt.db.query("SELECT name FROM users")
            if err { return "unknown" }
            return result
            crash { rt.db.query -> halt }
            test { self(Runtime { db: null }) == "alice" }
        }
        "#,
        r#"
            const rt = { db: { query: (sql) => ({ value: "alice", err: null }) } };
            console.log(get_user(rt));
        "#,
    ), "alice");
}

#[test]
fn adapter_pattern_error_propagation() {
    assert_eq!(run(
        r#"
        extern contract Database {
            query(sql: String) -> String, err {
                err failed = "query failed"
            }
        }

        pub struct Runtime {
            db: Database
        }{}

        pub fn get_user(rt: Runtime) -> String, err {
            let result, err = rt.db.query("SELECT 1")
            return result
            crash { rt.db.query -> log |> halt }
            test { self(Runtime { db: null }) == "ok" }
        }
        "#,
        r#"
            const rt = { db: { query: (sql) => ({ value: "", err: { name: "failed", message: "query failed" } }) } };
            const { value: val, err } = get_user(rt);
            console.log(err.name);
            console.log(err.message);
        "#,
    ), "failed\nquery failed");
}

#[test]
fn adapter_multiple_externs() {
    assert_eq!(run(
        r#"
        extern contract HttpClient {
            get(url: String) -> String, err {
                err network = "network error"
            }
        }

        extern contract Cache {
            get(key: String) -> String | null
        }

        pub struct Services {
            http: HttpClient
            cache: Cache
        }{}

        pub fn fetch_data(svc: Services, url: String) -> String {
            let cached, c_err = svc.cache.get(url)
            if cached != null { return cached }
            let result, h_err = svc.http.get(url)
            if h_err { return "error" }
            return result
            crash {
                svc.cache.get -> skip
                svc.http.get -> fallback("offline")
            }
            test { self(Services { http: null, cache: null }, "/api") == "data" }
        }
        "#,
        r#"
            const svc = {
                http: { get: (url) => ({ value: "hello from " + url, err: null }) },
                cache: { get: (key) => ({ value: null, err: null }) }
            };
            console.log(fetch_data(svc, "/api"));
        "#,
    ), "hello from /api");
}

#[test]
fn adapter_with_fallback() {
    assert_eq!(run(
        r#"
        extern contract Logger {
            info(msg: String) -> Ok
        }

        pub struct App {
            logger: Logger
        }{}

        pub fn greet(app: App, name: String) -> String {
            app.logger.info("greeting " + name)
            return "Hello " + name
            crash { app.logger.info -> skip }
            test { self(App { logger: null }, "cam") == "Hello cam" }
        }
        "#,
        r#"
            const app = { logger: { info: (msg) => ({ value: null, err: null }) } };
            console.log(greet(app, "world"));
        "#,
    ), "Hello world");
}
