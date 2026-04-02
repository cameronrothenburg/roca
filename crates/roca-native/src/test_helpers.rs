//! Shared test helpers for native JIT tests.

#[cfg(test)]
pub(super) use helpers::*;

#[cfg(test)]
mod helpers {
    pub use roca_cranelift::{JitModule, Module};
    pub use crate::{create_jit_module, compile_all, get_function_ptr};

    pub fn jit(source: &str) -> JitModule {
        let file = roca_parse::parse(source);
        let mut module = create_jit_module();
        compile_all(&mut *module, &file).unwrap();
        module.finalize().unwrap();
        module
    }

    pub unsafe fn call_f64(m: &JitModule, name: &str) -> *const u8 {
        get_function_ptr(m, name).unwrap()
    }

    /// Read a native string pointer as &str for test assertions.
    pub fn read_native_str(ptr: i64) -> &'static str {
        if ptr == 0 { return ""; }
        unsafe { std::ffi::CStr::from_ptr(ptr as *const i8) }
            .to_str()
            .unwrap_or("")
    }

    macro_rules! mem_test {
        ($name:ident, $body:block) => {
            #[test]
            fn $name() {
                crate::runtime::MEM.reset();
                $body
            }
        };
    }
    pub(crate) use mem_test;
}
