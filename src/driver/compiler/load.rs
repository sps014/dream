use super::*;

pub(super) struct LoadedProgram<'a> {
    pub acc: ProgramAccumulator<'a>,
    pub native_graph: crate::driver::native_sets::NativeGraph,
    pub cpp_bridge: crate::driver::ffi_shim::CppBridge,
}

impl Compiler {
    pub(super) fn load_program<'a>(
        &self,
        main_file_path: &String,
        arena: &'a Bump,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<LoadedProgram<'a>, CompileError> {
        let mut acc = ProgramAccumulator::default();
        parse_file_recursive(main_file_path, &mut acc, arena, diagnostics)?;

        let native_graph = crate::driver::native_sets::NativeGraph::load(main_file_path, &acc)
            .map_err(CompileError::Manifest)?;
        crate::driver::native_sets::resolve_bare_c_attrs(&mut acc, &native_graph, diagnostics);
        let cpp_bridge =
            crate::driver::ffi_shim::expand(arena, &mut acc, &native_graph, diagnostics)?;

        // Opt-in stdlib packages (`import system.net;`, etc.) plus always-on bootstrap
        // (`system.core` / `system.primitives`). `@json` types need `system.json` for derives.
        if program_uses_json_attr(&acc) {
            acc.requested_std_packages.insert("system.json".to_string());
        }
        if program_uses_gpu_shader_attr(&acc) {
            acc.requested_std_packages.insert("system.gpu".to_string());
        }
        merge_prelude(
            arena,
            &mut acc.all_functions,
            &mut acc.all_structs,
            &mut acc.all_interfaces,
            &mut acc.all_enums,
            &mut acc.all_extends,
            &mut acc.all_globals,
            diagnostics,
            &mut acc.file_contents,
            &mut acc.file_modules,
            &acc.requested_std_packages,
        )?;

        // Validate every attribute in the merged program (unknown names, disallowed placements,
        // wrong argument shapes, duplicates) before anything downstream (the `@json` derive below,
        // then semantic analysis) reads attributes assuming they are well-formed.
        dream_abi::attributes::validate_program_attributes(
            &acc.all_structs,
            &acc.all_interfaces,
            &acc.all_functions,
            &acc.all_enums,
            &acc.all_extends,
            diagnostics,
        );
        if diagnostics.has_errors() {
            return Err(fail_diagnostics(
                CompileError::Syntax,
                diagnostics,
                &acc.file_contents,
            ));
        }

        // Source generators: `@json` derive and registered `@generator`s (executed `GenContext`
        // bodies). Nested generator compiles set `skip_generators` so this cannot recurse.
        if !self.skip_generators {
            debug_assert!(
                !acc.all_structs.is_empty(),
                "run_generators must run after prelude merge / class collection"
            );
            run_generators(
                &self.toolchain_config,
                arena,
                &mut acc,
                main_file_path,
                diagnostics,
            )?;
        }

        // Inherit interface default-method bodies into implementing classes that omit them, by
        // appending synthesized `extend` blocks (must run after class collection so `implements`
        // clauses are all present).
        crate::driver::interface_defaults::generate_interface_default_impls(
            &acc.all_structs,
            &acc.all_interfaces,
            &mut acc.all_extends,
        );

        if diagnostics.has_errors() {
            return Err(fail_diagnostics(
                CompileError::Generator,
                diagnostics,
                &acc.file_contents,
            ));
        }

        Ok(LoadedProgram {
            acc,
            native_graph,
            cpp_bridge,
        })
    }
}
