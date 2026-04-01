//! Shared test helpers for native JIT tests.

#[cfg(test)]
pub(super) use helpers::*;

#[cfg(test)]
mod helpers {
    pub use cranelift_jit::JITModule;
    pub use cranelift_module::Module;
    pub use crate::native::{create_jit_module, compile_all};

    pub fn jit(source: &str) -> JITModule {
        let file = crate::parse::parse(source);
        let mut module = create_jit_module();
        compile_all(&mut module, &file).unwrap();
        module.finalize_definitions().unwrap();
        module
    }

    pub fn sig_f64(m: &JITModule, params: usize) -> cranelift_codegen::ir::Signature {
        let mut s = m.make_signature();
        for _ in 0..params {
            s.params.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        }
        s.returns.push(cranelift_codegen::ir::AbiParam::new(cranelift_codegen::ir::types::F64));
        s
    }

    pub unsafe fn call_f64(m: &mut JITModule, name: &str, params: usize) -> *const u8 {
        let sig = sig_f64(m, params);
        let id = m.declare_function(name, cranelift_module::Linkage::Export, &sig).unwrap();
        m.get_finalized_function(id)
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
                crate::native::runtime::MEM.reset();
                $body
            }
        };
    }
    pub(crate) use mem_test;
}
