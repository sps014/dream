//! The LLVM backend at `-O0` and `-O3` on a feature-spanning slice of the golden corpus, runtime
//! declarations in lockstep with `dream_rt.bc`, IR shapes, and byte-identical `.ll` across
//! compiles. Skipped (with a message) when the pinned LLVM toolchain is not installed.

use dream::driver::compiler::{Compiler, LlvmRuntimeRequest, LlvmToolchain};
use dream::driver::wasm_opt::OptLevel;
use dream::execution::llvm::{compile_llvm, resolve_llvm, Toolchain};
use dream::execution::native::{capture_native_bin, Pgo};
use dream_mir::backend::llvm::RuntimeSigs;
use dream_mir::backend::Target;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};

mod common;

/// One or more cases per plan area: arithmetic, recursion, loops, structs, arrays and a bounds
/// trap, classes, retain/release, destruction through `del`, ownership transfer, borrowed and
/// sink params, return ownership, interface dispatch, strings, List/Map, weak refs, async, tasks.
const CASES: &[&str] = &[
    "arithmetic",
    "functions",
    "for_each",
    "struct_basic",
    "struct_rc",
    "arrays",
    "array_alloc",
    "abc_post_loop_trap",
    "list_index_oob_trap",
    "classes",
    "class_methods",
    "arc_param",
    "arc_return",
    "arc_factory",
    "arc_same_arg_twice",
    "arc_global_reassign",
    "destroy_long_chain",
    "struct_last_use_move",
    "param_modes_borrow",
    "async_generic_sink_reuse",
    "interfaces",
    "interface_default_method",
    "generic_interfaces",
    "value_struct_interface_box",
    "strings",
    "string_builder",
    "string_interpolation",
    "list_basics",
    "map_basics",
    "union_match",
    "union_rc",
    "closure_capture",
    "closure_env_reclaim",
    "weak_field_runtime",
    "weak_handle_lifecycle",
    "async_basic",
    "async_value_struct_locals",
    "async_wide_scalar_return",
    "task_basic",
    "task_map",
];

fn llvm_available() -> bool {
    match resolve_llvm(&std::sync::Arc::new(
        dream::driver::toolchain::ToolchainConfig::default(),
    )) {
        Ok(_) => true,
        Err(e) => {
            eprintln!("skipping LLVM backend test: {e}");
            false
        }
    }
}

fn case(stem: &str) -> PathBuf {
    Path::new("tests/cases").join(format!("{stem}.dream"))
}

#[test]
fn unsigned_string_reads_do_not_depend_on_abi_extension_attributes() {
    use dream_mir::build::FunctionBuilder;
    use dream_mir::{Const, Mir, Operand, Place, Rvalue, Terminator};
    let interner = dream_types::TypeInterner::new();
    let mut builder = FunctionBuilder::new("unsigned_reads", interner.int());
    let source = builder.new_param(interner.string(), None);
    let char_value = builder.new_temp(interner.int());
    let byte_value = builder.new_temp(interner.int());
    let source = Operand::Copy(Place::Local(source));
    let index = Operand::Const(Const::Int(0));
    builder.assign(
        Place::Local(char_value),
        Rvalue::CharAt(source.clone(), index.clone(), true),
    );
    builder.assign(
        Place::Local(byte_value),
        Rvalue::ByteAt(source, index, true),
    );
    builder.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(
        byte_value,
    )))));
    let mir = Mir {
        functions: vec![builder.finish()],
        ..Default::default()
    };
    let target = dream_mir::backend::Target::native();
    let req = LlvmRuntimeRequest {
        need: dream_mir::runtime::runtime_need_from_mir(&mir),
        target: target.clone(),
        threads: false,
        wasm_opt: OptLevel::O0,
    };
    let toolchain = Toolchain {
        config: std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        opt: OptLevel::O0,
        debug: false,
    };
    let runtime = toolchain.runtime_sigs(&req).unwrap();
    let mut sigs = RuntimeSigs::parse(&runtime.text).unwrap();
    // MSVC's ABI omits zeroext even for unsigned narrow returns.
    for name in ["dream_char_at_u", "dream_byte_at_u"] {
        sigs.fns.get_mut(name).unwrap().ret_attrs.clear();
    }
    let ir = dream_mir::backend::llvm::emit_llvm_module(&mir, &interner, &sigs, false, target)
        .unwrap()
        .ir;
    let body = common::ir_func_body(&ir, "unsigned_reads");
    assert!(body.contains("zext i16"), "{}", body);
    assert!(body.contains("zext i8"), "{}", body);
    assert!(
        !body.contains("sext i16") && !body.contains("sext i8"),
        "{}",
        body
    );
}

fn out_dir(tag: &str) -> PathBuf {
    let dir = Path::new("target").join(tag);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn compile_ll(src: &Path, ll: &Path, opt: OptLevel) {
    Compiler::new(Target::native())
        .with_release(opt != OptLevel::O0)
        .with_llvm(std::sync::Arc::new(Toolchain {
            config: std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
            opt,
            debug: false,
        }))
        .compile(&src.display().to_string(), &ll.display().to_string())
        .unwrap_or_else(|e| panic!("LLVM compile failed for {}: {}", src.display(), e));
}

fn run_llvm(src: &Path, opt: OptLevel) -> Result<String, String> {
    let stem = src.file_stem().unwrap().to_str().unwrap();
    let ll = out_dir(&format!("llvm-backend-{opt:?}")).join(format!("{stem}.ll"));
    compile_ll(src, &ll, opt);
    let bin = compile_llvm(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        &ll,
        dream::execution::llvm::NativeBuildOptions {
            target: dream_abi::target::TargetSpec::host(),
            opt_ll: None,
            opt,
            debug: false,
            pgo: &Pgo::Off,
            icon: None,
            relocatable: false,
        },
    )
    .unwrap_or_else(|e| panic!("LLVM build failed for {}: {}", stem, e));
    capture_native_bin(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        &bin,
        ll.to_str().unwrap(),
        &[],
        &[],
        None,
        60,
    )
    .map(common::normalize_stdout)
    .map_err(|e| e.to_string())
}

#[test]
fn llvm_relocatable_binary_runs_after_move() {
    if !llvm_available() {
        return;
    }
    let temporary = tempfile::tempdir().unwrap();
    let build = temporary.path().join("build");
    fs::create_dir(&build).unwrap();
    let source = case("arithmetic");
    let ll = build.join("arithmetic.ll");
    compile_ll(&source, &ll, OptLevel::O0);
    let binary = compile_llvm(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        &ll,
        dream::execution::llvm::NativeBuildOptions {
            target: dream_abi::target::TargetSpec::host(),
            opt_ll: None,
            opt: OptLevel::O0,
            debug: false,
            pgo: &Pgo::Off,
            icon: None,
            relocatable: true,
        },
    )
    .unwrap();
    for capability in dream_abi::host_capability::HostCapability::ALL {
        assert_eq!(
            build.join(capability.library_name()).is_file(),
            capability == dream_abi::host_capability::HostCapability::Core
        );
    }
    let moved = temporary.path().join("moved package");
    fs::rename(&build, &moved).unwrap();
    let binary = moved.join(binary.file_name().unwrap());
    let output = std::process::Command::new(&binary)
        .env_remove("LD_LIBRARY_PATH")
        .env_remove("DYLD_LIBRARY_PATH")
        .current_dir(temporary.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        common::normalize_stdout(String::from_utf8_lossy(&output.stdout).into_owned()).trim(),
        fs::read_to_string(source.with_extension("expected"))
            .unwrap()
            .trim()
    );
}

fn check_case(stem: &str) -> Result<(), String> {
    let src = case(stem);
    let expected = fs::read_to_string(src.with_extension("expected")).ok();
    let trap = fs::read_to_string(src.with_extension("expected_trap")).ok();
    for opt in [OptLevel::O0, OptLevel::O3] {
        match (run_llvm(&src, opt), &expected, &trap) {
            (Ok(out), Some(want), None) => {
                if out.trim() != want.trim() {
                    return Err(format!("{stem} {opt:?}: stdout != .expected\n{out}"));
                }
            }
            (Err(err), _, Some(needle)) => {
                for line in needle.lines().map(str::trim).filter(|l| !l.is_empty()) {
                    if !err.contains(line) {
                        return Err(format!("{stem} {opt:?}: trap line `{line}` missing\n{err}"));
                    }
                }
            }
            (outcome, _, _) => {
                return Err(format!("{stem} {opt:?}: unexpected outcome {outcome:?}"))
            }
        }
    }
    Ok(())
}

#[test]
fn llvm_runs_feature_cases_at_o0_and_o3() {
    if !llvm_available() {
        return;
    }
    let failures: Vec<String> = CASES
        .par_iter()
        .filter_map(|s| check_case(s).err())
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}

#[test]
fn llvm_ir_is_deterministic() {
    if !llvm_available() {
        return;
    }
    for stem in ["classes", "async_basic", "interfaces", "union_rc"] {
        let dir = out_dir("llvm-backend-det");
        let texts: Vec<String> = (0..2)
            .map(|run| {
                let ll = dir.join(format!("{stem}_{run}.ll"));
                compile_ll(&case(stem), &ll, OptLevel::O3);
                fs::read_to_string(&ll).unwrap()
            })
            .collect();
        assert!(texts[0] == texts[1], "nondeterministic .ll for {}", stem);
    }
}

/// Every function the emitted module declares or exports that the runtime bitcode also has must
/// carry the runtime's exact type; a mismatch would link silently and call through the wrong ABI.
#[test]
fn llvm_runtime_declarations_match_bitcode() {
    if !llvm_available() {
        return;
    }
    let toolchain = Toolchain {
        config: std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        opt: OptLevel::O2,
        debug: false,
    };
    for stem in [
        "strings",
        "async_basic",
        "task_basic",
        "weak_handle_lifecycle",
        "map_basics",
    ] {
        let ll = out_dir("llvm-backend-lockstep").join(format!("{stem}.ll"));
        compile_ll(&case(stem), &ll, OptLevel::O2);
        let text = fs::read_to_string(&ll).unwrap();
        let need = dream_mir::runtime::runtime_need_from_module_text(&text);
        let req = LlvmRuntimeRequest {
            need,
            target: dream_mir::backend::Target::native(),
            threads: false,
            wasm_opt: OptLevel::O2,
        };
        let runtime = toolchain.runtime_sigs(&req).unwrap();
        let rt = RuntimeSigs::parse(&runtime.text).unwrap();
        let ours = RuntimeSigs::parse(&text).unwrap();
        assert_eq!(ours.triple, rt.triple, "{stem}: target triple");
        assert_eq!(ours.datalayout, rt.datalayout, "{stem}: datalayout");
        let mut checked = 0;
        for (name, f) in &ours.fns {
            if let Some(r) = rt.fns.get(name) {
                assert_eq!(f.fty, r.fty, "{stem}: `{name}` differs from dream_rt.bc");
                checked += 1;
            }
        }
        assert!(
            checked > 10,
            "{}: only {} runtime functions compared",
            stem,
            checked
        );
    }
}

fn function_body<'t>(ll: &'t str, name: &str) -> &'t str {
    let head = format!(" @{name}(");
    let start = ll
        .lines()
        .position(|l| l.starts_with("define ") && l.contains(&head))
        .unwrap_or_else(|| panic!("no definition of @{}", name));
    let from: usize = ll.lines().take(start).map(|l| l.len() + 1).sum();
    let len = ll[from..].find("\n}\n").expect("unterminated function");
    &ll[from..from + len]
}

/// Structural facts about the emitted module: constant dispatch tables, direct recursion, guarded
/// devirtualized interface calls, the split-tail release, a switch-dispatched async poll, division
/// guards only for non-constant divisors, caller-buffer value-struct returns with a boxing wrapper
/// for indirect callers, and no pointer attributes without a proof source.
#[test]
fn llvm_ir_shapes() {
    if !llvm_available() {
        return;
    }
    let ll_path = out_dir("llvm-backend-shapes").join("ir_shapes.ll");
    compile_ll(
        Path::new("tests/llvm/ir_shapes.dream"),
        &ll_path,
        OptLevel::O0,
    );
    let ll = fs::read_to_string(&ll_path).unwrap();

    assert!(ll.contains("@dream_ft = internal constant ["));
    assert!(ll.lines().any(|l| l.starts_with("@dream_iface_")
        && l.contains("internal constant")
        && l.contains("@s0_2_Sq_0_area")
        && l.contains("@s0_3_Tri_0_area")));
    assert!(!ll.contains("dream_init_ft") && !ll.contains("dream_init_itables"));

    assert!(function_body(&ll, "fib").matches("call i32 @fib(").count() == 2);

    let total = function_body(&ll, "total");
    assert!(total.contains("call i32 @s0_2_Sq_0_area(") && total.contains("call i32 @s0_3_Tri_0_area("));
    assert!(total.contains("@__iface_dispatch_"));
    assert!(total.contains("call i32 @dream_rc_last(") && total.contains("_into("));

    assert!(function_body(&ll, "poll_later").contains("switch i32"));

    let div_const = function_body(&ll, "div_const");
    assert!(div_const.contains("srem i32 %") && div_const.contains(", 64"));
    assert!(div_const.contains("sdiv i32 %") && div_const.contains(", 7"));
    assert!(!div_const.contains("icmp") && !div_const.contains("@dream_panic_at("));
    let div_var = function_body(&ll, "div_var");
    assert!(div_var.contains("icmp eq i32 %") && div_var.contains(", 0"));
    assert!(div_var.contains("@dream_panic_at(") && div_var.contains(", -1"));

    assert!(ll.contains("define internal void @mk(i32 %a0, ptr %a1)"));
    assert!(function_body(&ll, "mk__abi").contains("call void @mk("));
    assert!(ll.contains("call void @mk(i32 4, ptr "));
    assert!(ll
        .lines()
        .any(|l| l.starts_with("@dream_ft = ") && l.contains("ptr @mk__abi")));

    for l in ll.lines().filter(|l| l.starts_with("define ")) {
        assert!(l.contains("nounwind"), "missing nounwind: {}", l);
        for attr in [
            "noalias",
            "nonnull",
            "dereferenceable",
            "readonly",
            "noundef",
        ] {
            assert!(!l.contains(attr), "unproven `{}` on: {}", attr, l);
        }
    }
}

/// `--profile` then `--use-profile` through the pinned toolchain: the instrumented build records
/// a profile the pinned `llvm-profdata` merges, and the profile-optimized build behaves the same.
#[test]
#[ignore = "PGO round trip; cargo test --workspace -- --ignored"]
fn llvm_pgo_round_trip() {
    let Ok(tools) = resolve_llvm(&std::sync::Arc::new(
        dream::driver::toolchain::ToolchainConfig::default(),
    )) else {
        return;
    };
    if !tools.tool("llvm-profdata").is_file() {
        eprintln!("skipping: pinned LLVM has no llvm-profdata");
        return;
    }
    let src = case("for_each");
    let ll = out_dir("llvm-backend-pgo").join("for_each.ll");
    compile_ll(&src, &ll, OptLevel::O2);
    let expected = fs::read_to_string(src.with_extension("expected")).unwrap();
    let run = |bin: &Path| {
        capture_native_bin(
            &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
            bin,
            ll.to_str().unwrap(),
            &[],
            &[],
            None,
            60,
        )
        .map(common::normalize_stdout)
        .unwrap()
    };
    let gen = compile_llvm(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        &ll,
        dream::execution::llvm::NativeBuildOptions {
            target: dream_abi::target::TargetSpec::host(),
            opt_ll: None,
            opt: OptLevel::O2,
            debug: false,
            pgo: &Pgo::Generate,
            icon: None,
            relocatable: false,
        },
    )
    .unwrap();
    assert_eq!(run(&gen), expected);
    let raw = gen.with_extension("pgo");
    assert!(fs::read_dir(&raw)
        .unwrap()
        .flatten()
        .any(|e| e.path().extension().is_some_and(|x| x == "profraw")));
    let used = compile_llvm(
        &std::sync::Arc::new(dream::driver::toolchain::ToolchainConfig::default()),
        &ll,
        dream::execution::llvm::NativeBuildOptions {
            target: dream_abi::target::TargetSpec::host(),
            opt_ll: None,
            opt: OptLevel::O2,
            debug: false,
            pgo: &Pgo::Use(None),
            icon: None,
            relocatable: false,
        },
    )
    .unwrap();
    assert_eq!(run(&used), expected);
}
