use std::fmt::Write;

#[path = "abi_registry.rs"]
mod registry;

pub fn header() -> String {
    let mut out = String::from(
        "/* GENERATED from abi_registry.rs; run scripts/generate-abi.sh. */\n#ifndef DREAM_ABI_H\n#define DREAM_ABI_H\n#include <stdint.h>\n\n",
    );
    for &(name, value) in registry::ABI_NUMBERS {
        writeln!(out, "#define {name} {value}").unwrap();
    }
    for &(name, value) in registry::ABI_SYMBOLS {
        writeln!(out, "#define {name} {value:?}").unwrap();
    }
    let wasm = registry::FutureLayout::WASM32;
    write_layout(&mut out, "WASM", wasm);
    out.push_str("#if UINTPTR_MAX == UINT64_MAX\n");
    write_layout(
        &mut out,
        "NATIVE",
        registry::FutureLayout::compute(8, 8, true),
    );
    out.push_str("#elif UINTPTR_MAX == UINT32_MAX\n");
    write_layout(
        &mut out,
        "NATIVE",
        registry::FutureLayout::compute(4, 4, true),
    );
    out.push_str("#else\n#error Unsupported Dream pointer width\n#endif\n#ifdef DREAM_NATIVE\n");
    for name in FIELDS {
        writeln!(out, "#define F_{name} F_{name}_NATIVE").unwrap();
    }
    out.push_str("#else\n");
    for name in FIELDS {
        writeln!(out, "#define F_{name} F_{name}_WASM").unwrap();
    }
    out.push_str("#endif\n#endif\n");
    out
}

const FIELDS: &[&str] = &[
    "STATE",
    "STATUS",
    "RESULT",
    "POLL",
    "WAKER",
    "AWAITING",
    "KIND",
    "CHILDREN",
    "COUNT",
    "REMAINING",
    "RESULTS",
    "NEXT",
    "QUEUED",
    "DUE",
    "ESIZE",
    "WIDE",
    "SLOTS",
];

fn write_layout(out: &mut String, suffix: &str, f: registry::FutureLayout) {
    // wasm32 reuses the wide slot for its combinator element size.
    let esize = if suffix == "WASM" { f.wide } else { f.esize };
    let values = [
        f.state,
        f.status,
        f.result,
        f.poll,
        f.waker,
        f.awaiting,
        f.kind,
        f.children,
        f.count,
        f.remaining,
        f.results,
        f.next,
        f.queued,
        f.due,
        esize,
        f.wide,
        f.slots,
    ];
    for (name, value) in FIELDS.iter().zip(values) {
        writeln!(out, "#define F_{name}_{suffix} {value}").unwrap();
    }
}

pub fn javascript() -> String {
    let mut out = String::from(
        "// GENERATED from abi_registry.rs; run scripts/generate-abi.sh.\nexport const TAGS = {\n",
    );
    for &(name, value) in registry::ABI_NUMBERS {
        if let Some(tag) = name.strip_prefix("TAG_") {
            writeln!(out, "  {tag}: {value},").unwrap();
        }
    }
    writeln!(
        out,
        "}};\nexport const HEAP_HEADER_SIZE = {};",
        registry::HEAP_HEADER_SIZE
    )
    .unwrap();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_registered_constant_reaches_the_c_header() {
        let h = header();
        for &(name, value) in registry::ABI_NUMBERS {
            assert!(h.contains(&format!("#define {name} {value}\n")));
        }
        for &(name, value) in registry::ABI_SYMBOLS {
            assert!(h.contains(&format!("#define {name} {value:?}\n")));
        }
    }

    #[test]
    fn native_future_layout_is_selected_by_the_c_target_not_the_generator_host() {
        let h = header();
        assert!(h.contains("#if UINTPTR_MAX == UINT64_MAX"));
        assert!(h.contains("#elif UINTPTR_MAX == UINT32_MAX"));
        assert!(h.contains("#define F_SLOTS_NATIVE 104"));
        assert!(h.contains("#define F_SLOTS_NATIVE 72"));
        assert!(h.contains("#define F_SLOTS_WASM 64"));
    }
}
