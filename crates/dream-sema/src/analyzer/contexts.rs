use super::*;

impl<'a> Analyzer<'a> {
    /// Runs `f` with `current_generic_bindings` set to `bindings`, restoring the previous bindings
    /// afterward (even if `f` returns early via `?`). Replaces the manual "set then clear to empty"
    /// pattern at the monomorphized-body analysis sites, which both leaked bindings into the next
    /// body on an error path and clobbered (rather than restored) any enclosing bindings.
    pub(in crate::analyzer) fn with_generic_bindings<F, R>(
        &mut self,
        bindings: GenericBindings,
        f: F,
    ) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let saved = std::mem::replace(&mut self.current_generic_bindings, bindings);
        let result = f(self);
        self.current_generic_bindings = saved;
        result
    }

    /// Runs `f` with `current_function_is_async` set to `is_async`, restoring the previous value
    /// afterward so the flag cannot leak into a sibling function's analysis.
    pub(in crate::analyzer) fn with_async_flag<F, R>(&mut self, is_async: bool, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let saved = self.current_function_is_async;
        self.current_function_is_async = is_async;
        let result = f(self);
        self.current_function_is_async = saved;
        result
    }

    /// Runs `f` with `current_function_is_unsafe` set to `is_unsafe`, restoring the previous value
    /// afterward so the flag cannot leak into a sibling function's analysis.
    pub(in crate::analyzer) fn with_unsafe_flag<F, R>(&mut self, is_unsafe: bool, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let saved = self.current_function_is_unsafe;
        self.current_function_is_unsafe = is_unsafe;
        let result = f(self);
        self.current_function_is_unsafe = saved;
        result
    }

    /// Runs `f` with `current_function_runtime` set to `runtime`, restoring the previous value
    /// afterward so the flag cannot leak into a sibling function's analysis.
    pub(in crate::analyzer) fn with_runtime_flag<F, R>(
        &mut self,
        runtime: RuntimeSupport,
        f: F,
    ) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let saved = self.current_function_runtime;
        self.current_function_runtime = runtime;
        let result = f(self);
        self.current_function_runtime = saved;
        result
    }

    /// Runs `f` with GPU-stage flags set, restoring previous values afterward.
    pub(in crate::analyzer) fn with_gpu_flags<F, R>(
        &mut self,
        is_compute: bool,
        is_gpu: bool,
        f: F,
    ) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        let saved_c = self.current_function_is_compute;
        let saved_g = self.current_function_is_gpu;
        self.current_function_is_compute = is_compute;
        self.current_function_is_gpu = is_gpu;
        let result = f(self);
        self.current_function_is_compute = saved_c;
        self.current_function_is_gpu = saved_g;
        result
    }
}
