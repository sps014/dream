;; String payload layout (at the data pointer `ptr`, i.e. heap block + 12):
;;   [ptr+0]        byte length  : i32
;;   [ptr+4]        scalar length: i32  (Unicode scalar / code-point count, cached)
;;   [ptr+8 ..]     utf8 bytes
;; There is no NUL terminator: the length prefix makes it redundant, and every consumer (strlen,
;; string_eq, hashing, host interop) is length-driven. The 12-byte heap header ([size][tag][ref_count])
;; still lives at ptr-12 and is unchanged, so malloc/free/retain/release/object_tag are unaffected.
;; `size()` / `char_at` / iteration use Unicode scalar (code point) indices; `byte_size` / `byte_at`
;; expose raw UTF-8 byte access. `$str_scalar_len` is O(1) via the cached word at ptr+4.

;; Byte length of the UTF-8 payload (O(1)).
(func $str_byte_size (param $ptr i32) (result i32)
    local.get $ptr
    i32.load
)

;; Legacy name kept for concat and other byte-oriented callers.
(func $strlen (param $ptr i32) (result i32)
    local.get $ptr
    call $str_byte_size
)

;; UTF-8 width in bytes of the code point starting at byte offset `off` in `ptr`'s payload.
(func $utf8_width_at (param $ptr i32) (param $off i32) (result i32)
    (local $b i32)
    local.get $ptr
    i32.const 8
    i32.add
    local.get $off
    i32.add
    i32.load8_u
    local.set $b
    ;; ASCII
    local.get $b
    i32.const 0x80
    i32.lt_u
    if
        i32.const 1
        return
    end
    ;; 2-byte lead
    local.get $b
    i32.const 0xE0
    i32.and
    i32.const 0xC0
    i32.eq
    if
        i32.const 2
        return
    end
    ;; 3-byte lead
    local.get $b
    i32.const 0xF0
    i32.and
    i32.const 0xE0
    i32.eq
    if
        i32.const 3
        return
    end
    ;; 4-byte lead (and invalid sequences treated as width 1)
    i32.const 4
)

;; Decodes the Unicode scalar at byte offset `off` in `ptr`'s payload.
(func $utf8_decode_at (param $ptr i32) (param $off i32) (result i32)
    (local $b0 i32)
    (local $b1 i32)
    (local $b2 i32)
    (local $b3 i32)
    (local $base i32)
    local.get $ptr
    i32.const 8
    i32.add
    local.set $base
    local.get $base
    local.get $off
    i32.add
    i32.load8_u
    local.set $b0
    local.get $b0
    i32.const 0x80
    i32.lt_u
    if
        local.get $b0
        return
    end
    local.get $b0
    i32.const 0xE0
    i32.and
    i32.const 0xC0
    i32.eq
    if
        local.get $base
        local.get $off
        i32.const 1
        i32.add
        i32.add
        i32.load8_u
        local.set $b1
        local.get $b0
        i32.const 0x1F
        i32.and
        i32.const 6
        i32.shl
        local.get $b1
        i32.const 0x3F
        i32.and
        i32.or
        return
    end
    local.get $b0
    i32.const 0xF0
    i32.and
    i32.const 0xE0
    i32.eq
    if
        local.get $base
        local.get $off
        i32.const 1
        i32.add
        i32.add
        i32.load8_u
        local.set $b1
        local.get $base
        local.get $off
        i32.const 2
        i32.add
        i32.add
        i32.load8_u
        local.set $b2
        local.get $b0
        i32.const 0x0F
        i32.and
        i32.const 12
        i32.shl
        local.get $b1
        i32.const 0x3F
        i32.and
        i32.const 6
        i32.shl
        i32.or
        local.get $b2
        i32.const 0x3F
        i32.and
        i32.or
        return
    end
    local.get $base
    local.get $off
    i32.const 1
    i32.add
    i32.add
    i32.load8_u
    local.set $b1
    local.get $base
    local.get $off
    i32.const 2
    i32.add
    i32.add
    i32.load8_u
    local.set $b2
    local.get $base
    local.get $off
    i32.const 3
    i32.add
    i32.add
    i32.load8_u
    local.set $b3
    local.get $b0
    i32.const 0x07
    i32.and
    i32.const 18
    i32.shl
    local.get $b1
    i32.const 0x3F
    i32.and
    i32.const 12
    i32.shl
    i32.or
    local.get $b2
    i32.const 0x3F
    i32.and
    i32.const 6
    i32.shl
    i32.or
    local.get $b3
    i32.const 0x3F
    i32.and
    i32.or
)

;; Writes scalar `cp` at byte offset `off` in `ptr`'s payload; returns bytes written.
(func $utf8_encode_at (param $ptr i32) (param $off i32) (param $cp i32) (result i32)
    (local $base i32)
    local.get $ptr
    i32.const 8
    i32.add
    local.set $base
    local.get $cp
    i32.const 0x80
    i32.lt_u
    if
        local.get $base
        local.get $off
        i32.add
        local.get $cp
        i32.store8
        i32.const 1
        return
    end
    local.get $cp
    i32.const 0x800
    i32.lt_u
    if
        local.get $base
        local.get $off
        i32.add
        local.get $cp
        i32.const 6
        i32.shr_u
        i32.const 0xC0
        i32.or
        i32.store8
        local.get $base
        local.get $off
        i32.const 1
        i32.add
        i32.add
        local.get $cp
        i32.const 0x3F
        i32.and
        i32.const 0x80
        i32.or
        i32.store8
        i32.const 2
        return
    end
    local.get $cp
    i32.const 0x10000
    i32.lt_u
    if
        local.get $base
        local.get $off
        i32.add
        local.get $cp
        i32.const 12
        i32.shr_u
        i32.const 0xE0
        i32.or
        i32.store8
        local.get $base
        local.get $off
        i32.const 1
        i32.add
        i32.add
        local.get $cp
        i32.const 6
        i32.shr_u
        i32.const 0x3F
        i32.and
        i32.const 0x80
        i32.or
        i32.store8
        local.get $base
        local.get $off
        i32.const 2
        i32.add
        i32.add
        local.get $cp
        i32.const 0x3F
        i32.and
        i32.const 0x80
        i32.or
        i32.store8
        i32.const 3
        return
    end
    local.get $base
    local.get $off
    i32.add
    local.get $cp
    i32.const 18
    i32.shr_u
    i32.const 0xF0
    i32.or
    i32.store8
    local.get $base
    local.get $off
    i32.const 1
    i32.add
    i32.add
    local.get $cp
    i32.const 12
    i32.shr_u
    i32.const 0x3F
    i32.and
    i32.const 0x80
    i32.or
    i32.store8
    local.get $base
    local.get $off
    i32.const 2
    i32.add
    i32.add
    local.get $cp
    i32.const 6
    i32.shr_u
    i32.const 0x3F
    i32.and
    i32.const 0x80
    i32.or
    i32.store8
    local.get $base
    local.get $off
    i32.const 3
    i32.add
    i32.add
    local.get $cp
    i32.const 0x3F
    i32.and
    i32.const 0x80
    i32.or
    i32.store8
    i32.const 4
)

;; Cached Unicode scalar count (O(1)).
(func $str_scalar_len (param $ptr i32) (result i32)
    local.get $ptr
    i32.const 4
    i32.add
    i32.load
)

;; Counts Unicode scalars in a raw UTF-8 byte range starting at `base` (not a string data pointer).
(func $utf8_scalar_count (param $base i32) (param $byte_len i32) (result i32)
    (local $off i32)
    (local $count i32)
    (local $b i32)
    (local $w i32)
    i32.const 0
    local.set $off
    i32.const 0
    local.set $count
    (block $done
        (loop $scan
            local.get $off
            local.get $byte_len
            i32.ge_u
            br_if $done
            local.get $base
            local.get $off
            i32.add
            i32.load8_u
            local.set $b
            local.get $b
            i32.const 0x80
            i32.lt_u
            if
                i32.const 1
                local.set $w
            else
                local.get $b
                i32.const 0xE0
                i32.and
                i32.const 0xC0
                i32.eq
                if
                    i32.const 2
                    local.set $w
                else
                    local.get $b
                    i32.const 0xF0
                    i32.and
                    i32.const 0xE0
                    i32.eq
                    if
                        i32.const 3
                        local.set $w
                    else
                        i32.const 4
                        local.set $w
                    end
                end
            end
            local.get $off
            local.get $w
            i32.add
            local.set $off
            local.get $count
            i32.const 1
            i32.add
            local.set $count
            br $scan
        )
    )
    local.get $count
)

;; Byte offset of scalar index `idx` in `ptr`'s payload; returns `byte_len` when `idx` equals scalar count.
(func $utf8_scalar_byte_offset (param $ptr i32) (param $idx i32) (result i32)
    (local $byte_len i32)
    (local $off i32)
    (local $count i32)
    local.get $ptr
    call $str_byte_size
    local.set $byte_len
    i32.const 0
    local.set $off
    i32.const 0
    local.set $count
    (block $done
        (loop $scan
            local.get $count
            local.get $idx
            i32.eq
            br_if $done
            local.get $off
            local.get $byte_len
            i32.ge_u
            br_if $done
            local.get $off
            local.get $ptr
            local.get $off
            call $utf8_width_at
            i32.add
            local.set $off
            local.get $count
            i32.const 1
            i32.add
            local.set $count
            br $scan
        )
    )
    local.get $off
)

(func $concat_strings (param $str1 i32) (param $str2 i32) (result i32)
    (local $len1 i32)
    (local $len2 i32)
    (local $sc1 i32)
    (local $sc2 i32)
    (local $new_ptr i32)
    local.get $str1
    call $strlen
    local.set $len1
    local.get $str2
    call $strlen
    local.set $len2
    local.get $str1
    call $str_scalar_len
    local.set $sc1
    local.get $str2
    call $str_scalar_len
    local.set $sc2
  ;; size = 8 (byte_len + scalar_len) + len1 + len2
    local.get $len1
    local.get $len2
    i32.add
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $new_ptr
  ;; store combined byte_len at [new_ptr]
    local.get $new_ptr
    local.get $len1
    local.get $len2
    i32.add
    i32.store
  ;; store combined scalar_len at [new_ptr+4]
    local.get $new_ptr
    i32.const 4
    i32.add
    local.get $sc1
    local.get $sc2
    i32.add
    i32.store
  ;; memory.copy str1 payload -> new_ptr+8
    local.get $new_ptr
    i32.const 8
    i32.add
    local.get $str1
    i32.const 8
    i32.add
    local.get $len1
    memory.copy
  ;; memory.copy str2 payload -> new_ptr+8+len1
    local.get $new_ptr
    i32.const 8
    i32.add
    local.get $len1
    i32.add
    local.get $str2
    i32.const 8
    i32.add
    local.get $len2
    memory.copy
    local.get $new_ptr
)

(func $debug_get_free_list_head (result i32)
    global.get $free_list_head
)

(func $debug_get_heap_ptr (result i32)
    i32.const {HEAP_PTR_ADDR}
    i32.atomic.load
)

(func $debug_get_live_objects (result i32)
    global.get $live_objects
)

(func $debug_get_total_allocations (result i32)
    global.get $total_allocations
)

;; Reads the live reference count of a heap value (string/array/struct/object). The data pointer
;; passed in points just past the [size][tag][ref_count] header, so the count lives at ptr-4.
;; A null pointer reports 0.
(func $debug_get_ref_count (param $ptr i32) (result i32)
    local.get $ptr
    i32.eqz
    (if (result i32)
        (then i32.const 0)
        (else
            local.get $ptr
            i32.const 4
            i32.sub
            i32.load
        )
    )
)

(func $string_eq (param $a i32) (param $b i32) (result i32)
    (local $len i32)
    (local $i i32)
    (local $words i32)
  ;; identical pointers (covers the both-null case) are trivially equal
    local.get $a
    local.get $b
    i32.eq
    if
        i32.const 1
        return
    end
  ;; a null pointer can only equal another null pointer (handled above)
    local.get $a
    i32.eqz
    if
        i32.const 0
        return
    end
    local.get $b
    i32.eqz
    if
        i32.const 0
        return
    end
  ;; O(1) length mismatch check before comparing bytes
    local.get $a
    i32.load
    local.set $len
    local.get $len
    local.get $b
    i32.load
    i32.ne
    if
        i32.const 0
        return
    end
  ;; `memory.compare` is NOT available in our WASM target — word-wise i32 loads + byte tail.
    local.get $len
    i32.const 2
    i32.shr_u
    local.set $words
    i32.const 0
    local.set $i
    (block $words_done
        (loop $word_cmp
            local.get $i
            local.get $words
            i32.ge_u
            br_if $words_done
            local.get $a
            i32.const 8
            i32.add
            local.get $i
            i32.const 2
            i32.shl
            i32.add
            i32.load
            local.get $b
            i32.const 8
            i32.add
            local.get $i
            i32.const 2
            i32.shl
            i32.add
            i32.load
            i32.ne
            if
                i32.const 0
                return
            end
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $word_cmp
        )
    )
    local.get $words
    i32.const 2
    i32.shl
    local.set $i
    (block $tail_done
        (loop $tail
            local.get $i
            local.get $len
            i32.ge_u
            br_if $tail_done
            local.get $a
            i32.const 8
            i32.add
            local.get $i
            i32.add
            i32.load8_u
            local.get $b
            i32.const 8
            i32.add
            local.get $i
            i32.add
            i32.load8_u
            i32.ne
            if
                i32.const 0
                return
            end
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $tail
        )
    )
    i32.const 1
)

;; Scalar-indexed substring: clamp `[start, end)`, map to UTF-8 byte offsets, then one
;; `malloc` + `memory.copy`. Null `ptr` yields an empty string with zeroed headers.
(func $string_substring_raw (param $ptr i32) (param $start i32) (param $end i32) (result i32)
    (local $sc i32)
    (local $s i32)
    (local $e i32)
    (local $byte_start i32)
    (local $byte_end i32)
    (local $byte_len i32)
    (local $scalars i32)
    (local $p i32)
    local.get $ptr
    i32.eqz
    if
        i32.const 8
        i32.const {TAG_STRING}
        call $malloc
        local.set $p
        local.get $p
        i32.const 0
        i32.store
        local.get $p
        i32.const 4
        i32.add
        i32.const 0
        i32.store
        local.get $p
        return
    end
    local.get $ptr
    call $str_scalar_len
    local.set $sc
    local.get $start
    local.set $s
    local.get $s
    i32.const 0
    i32.lt_s
    if
        i32.const 0
        local.set $s
    end
    local.get $s
    local.get $sc
    i32.gt_u
    if
        local.get $sc
        local.set $s
    end
    local.get $end
    local.set $e
    local.get $e
    i32.const 0
    i32.lt_s
    if
        i32.const 0
        local.set $e
    end
    local.get $e
    local.get $sc
    i32.gt_u
    if
        local.get $sc
        local.set $e
    end
    local.get $e
    local.get $s
    i32.lt_u
    if
        local.get $s
        local.set $e
    end
    local.get $ptr
    local.get $s
    call $utf8_scalar_byte_offset
    local.set $byte_start
    local.get $ptr
    local.get $e
    call $utf8_scalar_byte_offset
    local.set $byte_end
    local.get $byte_end
    local.get $byte_start
    i32.sub
    local.set $byte_len
    local.get $e
    local.get $s
    i32.sub
    local.set $scalars
    local.get $byte_len
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $p
    local.get $p
    local.get $byte_len
    i32.store
    local.get $p
    i32.const 4
    i32.add
    local.get $scalars
    i32.store
    local.get $byte_len
    i32.eqz
    if
        local.get $p
        return
    end
    local.get $p
    i32.const 8
    i32.add
    local.get $ptr
    i32.const 8
    i32.add
    local.get $byte_start
    i32.add
    local.get $byte_len
    memory.copy
    local.get $p
)

;; Bulk-copy `count` UTF-8 bytes from string `src` into `byte[]` `dst`.
;; Source payload is at src+8; destination array payload is at dst+4.
(func $string_copy_utf8 (param $dst i32) (param $dst_off i32) (param $src i32) (param $src_off i32) (param $count i32)
    local.get $count
    i32.eqz
    br_if 0
    local.get $dst
    i32.eqz
    br_if 0
    local.get $src
    i32.eqz
    br_if 0
    local.get $dst
    i32.const 4
    i32.add
    local.get $dst_off
    i32.add
    local.get $src
    i32.const 8
    i32.add
    local.get $src_off
    i32.add
    local.get $count
    memory.copy
)

;; Unchecked scalar read; call sites emit a scalar-index bounds check first.
(func $char_at (param $ptr i32) (param $i i32) (result i32)
    (local $off i32)
    local.get $ptr
    local.get $i
    call $utf8_scalar_byte_offset
    local.set $off
    local.get $ptr
    local.get $off
    call $utf8_decode_at
)

;; Unchecked byte read; call sites emit a byte-index bounds check first.
(func $byte_at (param $ptr i32) (param $i i32) (result i32)
    local.get $ptr
    i32.const 8
    i32.add
    local.get $i
    i32.add
    i32.load8_u
)

;; Allocates an empty string buffer with room for up to `n` Unicode scalars (4*n payload bytes).
(func $string_alloc (param $n i32) (result i32)
    (local $p i32)
    local.get $n
    i32.const 4
    i32.mul
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $p
    local.get $p
    i32.const 0
    i32.store
    local.get $p
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    local.get $p
)

;; Builds a string from a `byte[]` data pointer (`[count:i32][payload...]`). Empty/null-safe.
(func $string_from_utf8 (param $bytes i32) (result i32)
    (local $count i32)
    (local $scalars i32)
    (local $p i32)
    local.get $bytes
    i32.eqz
    if
        i32.const 8
        i32.const {TAG_STRING}
        call $malloc
        local.set $p
        local.get $p
        i32.const 0
        i32.store
        local.get $p
        i32.const 4
        i32.add
        i32.const 0
        i32.store
        local.get $p
        return
    end
    local.get $bytes
    i32.load
    local.set $count
    local.get $count
    i32.eqz
    if
        i32.const 8
        i32.const {TAG_STRING}
        call $malloc
        local.set $p
        local.get $p
        i32.const 0
        i32.store
        local.get $p
        i32.const 4
        i32.add
        i32.const 0
        i32.store
        local.get $p
        return
    end
    local.get $bytes
    i32.const 4
    i32.add
    local.get $count
    call $utf8_scalar_count
    local.set $scalars
    local.get $count
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $p
    local.get $p
    local.get $count
    i32.store
    local.get $p
    i32.const 4
    i32.add
    local.get $scalars
    i32.store
    local.get $p
    i32.const 8
    i32.add
    local.get $bytes
    i32.const 4
    i32.add
    local.get $count
    memory.copy
    local.get $p
)

;; Writes scalar `c` at scalar index `i`, appending when `i` equals the current scalar count.
(func $string_set (param $ptr i32) (param $i i32) (param $c i32)
    (local $scalar_len i32)
    (local $byte_off i32)
    (local $old_w i32)
    (local $new_w i32)
    (local $byte_len i32)
    (local $tail i32)
    local.get $ptr
    call $str_scalar_len
    local.set $scalar_len
    local.get $i
    local.get $scalar_len
    i32.gt_u
    if
        unreachable
    end
    ;; UTF-8 encoded width of `c` (without writing).
    local.get $c
    i32.const 0x80
    i32.lt_u
    if
        i32.const 1
        local.set $new_w
    else
        local.get $c
        i32.const 0x800
        i32.lt_u
        if
            i32.const 2
            local.set $new_w
        else
            local.get $c
            i32.const 0x10000
            i32.lt_u
            if
                i32.const 3
                local.set $new_w
            else
                i32.const 4
                local.set $new_w
            end
        end
    end
    local.get $i
    local.get $scalar_len
    i32.eq
    if
        local.get $ptr
        call $str_byte_size
        local.set $byte_len
        local.get $ptr
        local.get $byte_len
        local.get $c
        call $utf8_encode_at
        drop
        local.get $ptr
        local.get $byte_len
        local.get $new_w
        i32.add
        i32.store
        local.get $ptr
        i32.const 4
        i32.add
        local.get $scalar_len
        i32.const 1
        i32.add
        i32.store
        return
    end
    local.get $ptr
    local.get $i
    call $utf8_scalar_byte_offset
    local.set $byte_off
    local.get $ptr
    local.get $byte_off
    call $utf8_width_at
    local.set $old_w
    local.get $old_w
    local.get $new_w
    i32.ne
    if
        local.get $ptr
        call $str_byte_size
        local.set $byte_len
        local.get $byte_len
        local.get $byte_off
        i32.sub
        local.get $old_w
        i32.sub
        local.set $tail
        ;; memory.copy handles overlapping regions like memmove.
        local.get $ptr
        i32.const 8
        i32.add
        local.get $byte_off
        i32.add
        local.get $new_w
        i32.add
        local.get $ptr
        i32.const 8
        i32.add
        local.get $byte_off
        i32.add
        local.get $old_w
        i32.add
        local.get $tail
        memory.copy
        local.get $ptr
        local.get $byte_len
        local.get $new_w
        i32.add
        local.get $old_w
        i32.sub
        i32.store
    end
    local.get $ptr
    local.get $byte_off
    local.get $c
    call $utf8_encode_at
    drop
)

(func $string_from_utf8_prefix (param $bytes i32) (param $len i32) (result i32)
    (local $count i32)
    (local $scalars i32)
    (local $p i32)
    local.get $bytes
    i32.eqz
    if
        i32.const 8
        i32.const {TAG_STRING}
        call $malloc
        local.set $p
        local.get $p
        i32.const 0
        i32.store
        local.get $p
        i32.const 4
        i32.add
        i32.const 0
        i32.store
        local.get $p
        return
    end
    local.get $bytes
    i32.load
    local.set $count
    local.get $len
    i32.const 0
    i32.lt_s
    if
        i32.const 0
        local.set $len
    end
    local.get $len
    local.get $count
    i32.gt_s
    if
        local.get $count
        local.set $len
    end
    local.get $len
    i32.eqz
    if
        i32.const 8
        i32.const {TAG_STRING}
        call $malloc
        local.set $p
        local.get $p
        i32.const 0
        i32.store
        local.get $p
        i32.const 4
        i32.add
        i32.const 0
        i32.store
        local.get $p
        return
    end
    local.get $bytes
    i32.const 4
    i32.add
    local.get $len
    call $utf8_scalar_count
    local.set $scalars
    local.get $len
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $p
    local.get $p
    local.get $len
    i32.store
    local.get $p
    i32.const 4
    i32.add
    local.get $scalars
    i32.store
    local.get $p
    i32.const 8
    i32.add
    local.get $bytes
    i32.const 4
    i32.add
    local.get $len
    memory.copy
    local.get $p
)
