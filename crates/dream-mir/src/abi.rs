//! Runtime ABI shared by MIR, C guests and JS hosts.

include!("../abi_registry.rs");

/// Runtime string hash (`dream_string_hash_slow`): FNV-1a over UTF-16 units, folded away from
/// the pad markers so it can be cached in the pad word. The emitter stores it into static
/// literal blocks so the runtime never has to write them.
pub fn string_hash(units: &[u16]) -> i32 {
    let mut h: u32 = 2_166_136_261;
    for &u in units {
        h ^= u32::from(u);
        h = h.wrapping_mul(16_777_619);
    }
    if h <= 1 {
        h += 2;
    }
    h as i32
}

/// Pointer width / alignment for a backend, plus the layouts that depend on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TargetAbi {
    pub ptr_size: u32,
    pub ptr_align: u32,
    pub heap_header_size: u32,
    pub future: FutureLayout,
}

impl TargetAbi {
    pub const WASM32: Self = Self {
        ptr_size: 4,
        ptr_align: 4,
        heap_header_size: HEAP_HEADER_SIZE,
        future: FutureLayout::WASM32,
    };

    pub fn for_target(spec: &dream_abi::target::TargetSpec) -> Self {
        Self {
            ptr_size: spec.ptr_size,
            ptr_align: spec.ptr_align,
            heap_header_size: if spec.capabilities.linear_memory {
                HEAP_HEADER_SIZE
            } else {
                NATIVE_HEAP_HEADER_SIZE
            },
            future: FutureLayout::for_target(spec),
        }
    }
}

/// Byte offsets of the `Future` frame header. WASM uses packed i32 fields ([`FutureLayout::WASM32`]);
/// native packs the same fields with host pointer size so `F_WIDE` never aliases `F_REMAINING`.
impl FutureLayout {
    pub fn for_target(spec: &dream_abi::target::TargetSpec) -> Self {
        Self::compute(
            spec.ptr_size,
            spec.ptr_align,
            !spec.capabilities.linear_memory,
        )
    }
}

/// Address of the refcount word given a data pointer.
pub fn rc_addr(data_ptr: u32) -> u32 {
    data_ptr.wrapping_sub(RC_FROM_DATA)
}

/// Address of the first array element given an array data pointer (`[len][payload…]`).
pub fn elem_base(array_ptr: u32) -> u32 {
    array_ptr.wrapping_add(LEN_PREFIX_SIZE)
}

pub const ENV_PRINT_IMPORTS: &[(&str, PrintVal)] = &[
    (PRINT_STRING, PrintVal::I32),
    (PRINT_INT, PrintVal::I32),
    (PRINT_FLOAT, PrintVal::F32),
    (PRINT_DOUBLE, PrintVal::F64),
    (PRINT_CHAR, PrintVal::I32),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrintVal {
    I32,
    F32,
    F64,
}

#[cfg(test)]
mod abi_h_lockstep {
    use super::*;

    fn header_define(src: &str, name: &str) -> i64 {
        for line in src.lines() {
            let line = line.trim();
            let prefix = format!("#define {name} ");
            if let Some(rest) = line.strip_prefix(&prefix) {
                let rest = rest.trim();
                if rest.starts_with('(') {
                    // SHADOW_STACK_SIZE (16 * WASM_PAGE_SIZE) — skip compound forms.
                    continue;
                }
                return rest
                    .parse()
                    .unwrap_or_else(|_| panic!("bad #define {}: {}", name, rest));
            }
        }
        panic!("missing #define {} in dream_abi.h", name);
    }

    #[test]
    fn string_hash_is_fnv1a_folded_off_pad_markers() {
        assert_eq!(string_hash(&[]) as u32, 0x811c_9dc5);
        assert_eq!(string_hash(&[u16::from(b'a')]) as u32, 0xe40c_292c);
        let obj = include_str!("runtime/c/native/object.c");
        assert!(obj.contains("2166136261u") && obj.contains("16777619u"));
        for units in [&[][..], &[1, 2, 3], &[0xffff; 9]] {
            assert!(!matches!(
                string_hash(units),
                DREAM_STR_PAD_INLINE | DREAM_STR_SLICE
            ));
        }
    }

    #[test]
    fn dream_abi_h_matches_abi_rs() {
        let h = include_str!("runtime/c/include/dream_abi.h");
        assert_eq!(header_define(h, "TAG_INT"), TAG_INT as i64);
        assert_eq!(header_define(h, "TAG_FLOAT"), TAG_FLOAT as i64);
        assert_eq!(header_define(h, "TAG_DOUBLE"), TAG_DOUBLE as i64);
        assert_eq!(header_define(h, "TAG_BOOL"), TAG_BOOL as i64);
        assert_eq!(header_define(h, "TAG_STRING"), TAG_STRING as i64);
        assert_eq!(header_define(h, "TAG_ARRAY"), TAG_ARRAY as i64);
        assert_eq!(header_define(h, "TAG_CHAR"), TAG_CHAR as i64);
        assert_eq!(header_define(h, "TAG_LONG"), TAG_LONG as i64);
        assert_eq!(header_define(h, "TAG_UINT"), TAG_UINT as i64);
        assert_eq!(header_define(h, "TAG_ULONG"), TAG_ULONG as i64);
        assert_eq!(header_define(h, "TAG_BYTE"), TAG_BYTE as i64);
        assert_eq!(header_define(h, "TAG_ISIZE"), TAG_ISIZE as i64);
        assert_eq!(header_define(h, "TAG_USIZE"), TAG_USIZE as i64);
        assert_eq!(header_define(h, "TAG_FUTURE"), TAG_FUTURE as i64);
        assert_eq!(header_define(h, "TAG_FUNCBOX"), TAG_FUNCBOX as i64);
        assert_eq!(header_define(h, "TAG_CLOSURE_ENV"), TAG_CLOSURE_ENV as i64);
        assert_eq!(header_define(h, "TAG_STRUCT_BASE"), TAG_STRUCT_BASE as i64);
        assert_eq!(header_define(h, "TAG_SHARED"), TAG_SHARED as i64);
        assert_eq!(header_define(h, "TAG_VALUE_MASK"), TAG_VALUE_MASK as i64);
        assert_eq!(
            header_define(h, "HEAP_HEADER_SIZE"),
            HEAP_HEADER_SIZE as i64
        );
        assert_eq!(
            header_define(h, "HEADER_TAG_OFFSET"),
            HEADER_TAG_OFFSET as i64
        );
        assert_eq!(
            header_define(h, "HEADER_REFCOUNT_OFFSET"),
            HEADER_REFCOUNT_OFFSET as i64
        );
        assert_eq!(header_define(h, "LEN_PREFIX_SIZE"), LEN_PREFIX_SIZE as i64);
        assert_eq!(
            header_define(h, "STRING_HEADER_SIZE"),
            STRING_HEADER_SIZE as i64
        );
        assert_eq!(
            header_define(h, "STRING_UNITS_OFFSET"),
            STRING_UNITS_OFFSET as i64
        );
        assert_eq!(
            header_define(h, "STRING_SCALAR_LEN_OFFSET"),
            STRING_SCALAR_LEN_OFFSET as i64
        );
        assert_eq!(header_define(h, "WASM_PAGE_SIZE"), WASM_PAGE_SIZE as i64);
        assert_eq!(
            header_define(h, "INITIAL_HEAP_PAGES"),
            INITIAL_HEAP_PAGES as i64
        );
        assert_eq!(
            header_define(h, "MAX_MEMORY_PAGES"),
            MAX_MEMORY_PAGES as i64
        );
        assert_eq!(header_define(h, "STRING_BASE"), STRING_BASE as i64);
        assert_eq!(
            header_define(h, "DREAM_REGEX_IGNORE_CASE"),
            DREAM_REGEX_IGNORE_CASE as i64
        );
        assert_eq!(
            header_define(h, "DREAM_REGEX_MULTILINE"),
            DREAM_REGEX_MULTILINE as i64
        );
        assert_eq!(
            header_define(h, "DREAM_REGEX_DOTALL"),
            DREAM_REGEX_DOTALL as i64
        );
        assert_eq!(header_define(h, "ALLOC_LOCK_ADDR"), ALLOC_LOCK_ADDR as i64);
        assert_eq!(header_define(h, "HEAP_PTR_ADDR"), HEAP_PTR_ADDR as i64);
        assert_eq!(
            header_define(h, "THREAD_ID_COUNTER_ADDR"),
            THREAD_ID_COUNTER_ADDR as i64
        );
        assert_eq!(
            header_define(h, "ASYNC_RQ_HEAD_ADDR"),
            ASYNC_RQ_HEAD_ADDR as i64
        );
        assert_eq!(
            header_define(h, "ASYNC_RQ_TAIL_ADDR"),
            ASYNC_RQ_TAIL_ADDR as i64
        );
        assert_eq!(
            header_define(h, "ASYNC_TIMER_HEAD_ADDR"),
            ASYNC_TIMER_HEAD_ADDR as i64
        );
        assert_eq!(
            header_define(h, "ASYNC_VCLOCK_ADDR"),
            ASYNC_VCLOCK_ADDR as i64
        );
        assert_eq!(
            header_define(h, "HEADER_LOCK_WORD_SIZE"),
            HEADER_LOCK_WORD_SIZE as i64
        );
        assert_eq!(header_define(h, "LOCK_DEPTH_BITS"), LOCK_DEPTH_BITS as i64);
        assert_eq!(
            header_define(h, "NATIVE_HEAP_HEADER_SIZE"),
            NATIVE_HEAP_HEADER_SIZE as i64
        );
        assert_eq!(
            header_define(h, "DREAM_STR_PAD_INLINE"),
            DREAM_STR_PAD_INLINE as i64
        );
        assert_eq!(header_define(h, "DREAM_STR_SLICE"), DREAM_STR_SLICE as i64);
        assert_eq!(header_define(h, "RC_FROM_DATA"), RC_FROM_DATA as i64);
        assert_eq!(header_define(h, "TAG_FROM_DATA"), TAG_FROM_DATA as i64);
        let w = FutureLayout::WASM32;
        assert_eq!(w.state, 0);
        assert_eq!(w.status, 4);
        assert_eq!(w.result, 8);
        assert_eq!(w.poll, 12);
        assert_eq!(w.waker, 16);
        assert_eq!(w.awaiting, 20);
        assert_eq!(w.kind, 24);
        assert_eq!(w.children, 28);
        assert_eq!(w.count, 32);
        assert_eq!(w.remaining, 36);
        assert_eq!(w.results, 40);
        assert_eq!(w.next, 44);
        assert_eq!(w.queued, 48);
        assert_eq!(w.due, 52);
        assert_eq!(w.wide, 56);
        assert_eq!(w.slots, 64);
        assert_eq!(w.esize, 0);
        assert_ne!(w.wide, w.remaining);
        for (name, want) in [
            ("F_STATE_WASM", w.state),
            ("F_STATUS_WASM", w.status),
            ("F_RESULT_WASM", w.result),
            ("F_POLL_WASM", w.poll),
            ("F_WAKER_WASM", w.waker),
            ("F_AWAITING_WASM", w.awaiting),
            ("F_KIND_WASM", w.kind),
            ("F_CHILDREN_WASM", w.children),
            ("F_COUNT_WASM", w.count),
            ("F_REMAINING_WASM", w.remaining),
            ("F_RESULTS_WASM", w.results),
            ("F_NEXT_WASM", w.next),
            ("F_QUEUED_WASM", w.queued),
            ("F_DUE_WASM", w.due),
            ("F_WIDE_WASM", w.wide),
            ("F_SLOTS_WASM", w.slots),
        ] {
            assert_eq!(header_define(h, name), want as i64, "{name}");
        }
        let n = FutureLayout::for_target(
            &dream_abi::target::TargetSpec::parse("x86_64-unknown-linux-gnu").unwrap(),
        );
        assert_ne!(n.wide, n.remaining);
        assert!(n.wide + 8 <= n.slots);
        assert!(n.esize + 4 <= n.wide || n.wide + 8 <= n.esize);
        for (name, want) in [
            ("F_STATE_NATIVE", n.state),
            ("F_STATUS_NATIVE", n.status),
            ("F_RESULT_NATIVE", n.result),
            ("F_POLL_NATIVE", n.poll),
            ("F_WAKER_NATIVE", n.waker),
            ("F_AWAITING_NATIVE", n.awaiting),
            ("F_KIND_NATIVE", n.kind),
            ("F_CHILDREN_NATIVE", n.children),
            ("F_COUNT_NATIVE", n.count),
            ("F_REMAINING_NATIVE", n.remaining),
            ("F_RESULTS_NATIVE", n.results),
            ("F_NEXT_NATIVE", n.next),
            ("F_QUEUED_NATIVE", n.queued),
            ("F_DUE_NATIVE", n.due),
            ("F_ESIZE_NATIVE", n.esize),
            ("F_WIDE_NATIVE", n.wide),
            ("F_SLOTS_NATIVE", n.slots),
        ] {
            assert_eq!(header_define(h, name), want as i64, "{name}");
        }
        assert_eq!(
            SHADOW_STACK_SIZE,
            16 * WASM_PAGE_SIZE,
            "keep dream_abi.h SHADOW_STACK_SIZE in sync"
        );
    }

    #[test]
    fn core_js_tags_match_abi_rs() {
        let js = include_str!("../../../runtime/src/abi.js");
        fn js_num(src: &str, key: &str) -> i64 {
            let pat = format!("{key}:");
            for line in src.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix(&pat) {
                    let n = rest.trim().trim_end_matches(',');
                    return n.parse().unwrap_or_else(|_| panic!("bad {}: {}", key, n));
                }
            }
            panic!("missing {} in JS", key);
        }
        fn js_assign(src: &str, name: &str) -> i64 {
            let pat = format!("export const {name} = ");
            for line in src.lines() {
                let line = line.trim();
                if let Some(rest) = line.strip_prefix(&pat) {
                    let n = rest.split(['/', ';']).next().unwrap().trim();
                    return n.parse().unwrap_or_else(|_| panic!("bad {}: {}", name, n));
                }
            }
            panic!("missing {} in JS", name);
        }
        assert_eq!(js_num(js, "INT"), TAG_INT as i64);
        assert_eq!(js_num(js, "FLOAT"), TAG_FLOAT as i64);
        assert_eq!(js_num(js, "DOUBLE"), TAG_DOUBLE as i64);
        assert_eq!(js_num(js, "BOOL"), TAG_BOOL as i64);
        assert_eq!(js_num(js, "STRING"), TAG_STRING as i64);
        assert_eq!(js_num(js, "ARRAY"), TAG_ARRAY as i64);
        assert_eq!(js_num(js, "CHAR"), TAG_CHAR as i64);
        assert_eq!(js_num(js, "LONG"), TAG_LONG as i64);
        assert_eq!(js_num(js, "UINT"), TAG_UINT as i64);
        assert_eq!(js_num(js, "ULONG"), TAG_ULONG as i64);
        assert_eq!(js_num(js, "ISIZE"), TAG_ISIZE as i64);
        assert_eq!(js_num(js, "USIZE"), TAG_USIZE as i64);
        assert_eq!(js_num(js, "BYTE"), TAG_BYTE as i64);
        assert_eq!(js_num(js, "STRUCT_BASE"), TAG_STRUCT_BASE as i64);
        assert_eq!(js_assign(js, "HEAP_HEADER_SIZE"), HEAP_HEADER_SIZE as i64);
    }
}
