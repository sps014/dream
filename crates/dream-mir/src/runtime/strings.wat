;; String payload layout (at the data pointer `ptr`, i.e. heap block + 12):
;;   [ptr+0]        unit length  : i32  (UTF-16 code-unit count = `.length`)
;;   [ptr+4]        pad          : i32  (unused; keeps an 8-byte header)
;;   [ptr+8 ..]     UTF-16 LE units
;; There is no NUL terminator: the length prefix makes it redundant, and every consumer (strlen,
;; string_eq, hashing, host interop) is length-driven. The 12-byte heap header ([size][tag][ref_count])
;; still lives at ptr-12 and is unchanged, so malloc/free/retain/release/object_tag are unaffected.
;; `char_at` / iteration index UTF-16 code units; `byte_size` is `unit_len * 2`; `byte_at` is raw LE.

;; Payload byte length (O(1)): UTF-16 unit count times two.
(func $str_byte_size (param $ptr i32) (result i32)
    local.get $ptr
    i32.load
    i32.const 1
    i32.shl
)

;; Legacy name kept for concat and other byte-oriented callers.
(func $strlen (param $ptr i32) (result i32)
    local.get $ptr
    call $str_byte_size
)

;; UTF-16 unit width in payload bytes (always 2). Kept as `$utf8_width_at` for the stdlib intrinsic.
(func $utf8_width_at (param $ptr i32) (param $off i32) (result i32)
    i32.const 2
)

;; UTF-16 code unit at byte offset `off` in the payload (even offsets).
(func $utf8_decode_at (param $ptr i32) (param $off i32) (result i32)
    local.get $ptr
    i32.const 8
    i32.add
    local.get $off
    i32.add
    i32.load16_u
)

;; UTF-8 width in a raw UTF-8 buffer starting at `base` (used by `$string_from_utf8`).
(func $utf8_width_raw (param $base i32) (param $off i32) (result i32)
    (local $b i32)
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
        return
    end
    local.get $b
    i32.const 0xE0
    i32.and
    i32.const 0xC0
    i32.eq
    if
        i32.const 2
        return
    end
    local.get $b
    i32.const 0xF0
    i32.and
    i32.const 0xE0
    i32.eq
    if
        i32.const 3
        return
    end
    i32.const 4
)

;; Decode one Unicode scalar from raw UTF-8 at `base+off`.
(func $utf8_decode_raw (param $base i32) (param $off i32) (result i32)
    (local $b0 i32)
    (local $b1 i32)
    (local $b2 i32)
    (local $b3 i32)
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

;; Write Unicode scalar `cp` as UTF-16 at payload `dst` unit index `i`. Returns units written (1 or 2).
(func $utf16_encode_at (param $dst i32) (param $i i32) (param $cp i32) (result i32)
    local.get $cp
    i32.const 0x10000
    i32.lt_u
    if
        local.get $dst
        local.get $i
        i32.const 1
        i32.shl
        i32.add
        local.get $cp
        i32.store16
        i32.const 1
        return
    end
    ;; surrogate pair
    local.get $dst
    local.get $i
    i32.const 1
    i32.shl
    i32.add
    local.get $cp
    i32.const 0x10000
    i32.sub
    i32.const 10
    i32.shr_u
    i32.const 0xD800
    i32.add
    i32.store16
    local.get $dst
    local.get $i
    i32.const 1
    i32.add
    i32.const 1
    i32.shl
    i32.add
    local.get $cp
    i32.const 0x10000
    i32.sub
    i32.const 0x3FF
    i32.and
    i32.const 0xDC00
    i32.add
    i32.store16
    i32.const 2
)

;; UTF-16 code-unit count (O(1)).
(func $str_scalar_len (param $ptr i32) (result i32)
    local.get $ptr
    i32.load
)

(func $concat_strings (param $str1 i32) (param $str2 i32) (result i32)
    (local $len1 i32)
    (local $len2 i32)
    (local $sc1 i32)
    (local $sc2 i32)
    (local $new_ptr i32)
    local.get $str1
    i32.load
    local.set $sc1
    local.get $sc1
    i32.const 1
    i32.shl
    local.set $len1
    local.get $str2
    i32.load
    local.set $sc2
    local.get $sc2
    i32.const 1
    i32.shl
    local.set $len2
    local.get $len1
    i32.eqz
    if
        local.get $len2
        i32.eqz
        if
            global.get $__rt_str_empty
            call $retain
            global.get $__rt_str_empty
            return
        end
        ;; Identity return still transfers an owned ref to the caller.
        local.get $str2
        call $retain
        local.get $str2
        return
    end
    local.get $len2
    i32.eqz
    if
    local.get $str1
    call $retain
    local.get $str1
    return
    end
  ;; size = 8 (unit_len + pad) + utf16 payload bytes
    local.get $len1
    local.get $len2
    i32.add
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $new_ptr
  ;; store combined unit_len at [new_ptr]
    local.get $new_ptr
    local.get $sc1
    local.get $sc2
    i32.add
    i32.store
  ;; pad
    local.get $new_ptr
    i32.const 4
    i32.add
    i32.const 0
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

;; Three-way concat: one malloc + three copies (avoids the temp from nested `$concat_strings`).
(func $concat_strings3 (param $str1 i32) (param $str2 i32) (param $str3 i32) (result i32)
    (local $len1 i32)
    (local $len2 i32)
    (local $len3 i32)
    (local $sc1 i32)
    (local $sc2 i32)
    (local $sc3 i32)
    (local $new_ptr i32)
    (local $off i32)
    local.get $str1
    i32.load
    local.set $sc1
    local.get $sc1
    i32.const 1
    i32.shl
    local.set $len1
    local.get $str2
    i32.load
    local.set $sc2
    local.get $sc2
    i32.const 1
    i32.shl
    local.set $len2
    local.get $str3
    i32.load
    local.set $sc3
    local.get $sc3
    i32.const 1
    i32.shl
    local.set $len3
    local.get $len1
    local.get $len2
    i32.add
    local.get $len3
    i32.add
    i32.eqz
    if
        global.get $__rt_str_empty
        call $retain
        global.get $__rt_str_empty
        return
    end
    ;; Degenerate: if two sides are empty, fall back to pairwise retain/concat.
    local.get $len1
    i32.eqz
    if
        local.get $str2
        local.get $str3
        call $concat_strings
        return
    end
    local.get $len2
    i32.eqz
    if
        local.get $str1
        local.get $str3
        call $concat_strings
        return
    end
    local.get $len3
    i32.eqz
    if
        local.get $str1
        local.get $str2
        call $concat_strings
        return
    end
    local.get $len1
    local.get $len2
    i32.add
    local.get $len3
    i32.add
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $new_ptr
    local.get $new_ptr
    local.get $sc1
    local.get $sc2
    i32.add
    local.get $sc3
    i32.add
    i32.store
    local.get $new_ptr
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    local.get $new_ptr
    i32.const 8
    i32.add
    local.get $str1
    i32.const 8
    i32.add
    local.get $len1
    memory.copy
    local.get $len1
    local.set $off
    local.get $new_ptr
    i32.const 8
    i32.add
    local.get $off
    i32.add
    local.get $str2
    i32.const 8
    i32.add
    local.get $len2
    memory.copy
    local.get $off
    local.get $len2
    i32.add
    local.set $off
    local.get $new_ptr
    i32.const 8
    i32.add
    local.get $off
    i32.add
    local.get $str3
    i32.const 8
    i32.add
    local.get $len3
    memory.copy
    local.get $new_ptr
)

;; `"pref" + int + "suf"` in one malloc — no intermediate `$int_to_string` string.
(func $concat_str_int_str (param $pref i32) (param $v i32) (param $suf i32) (result i32)
    (local $plen i32)
    (local $slen i32)
    (local $ndigits i32)
    (local $neg i32)
    (local $tmp i32)
    (local $digit i32)
    (local $total i32)
    (local $p i32)
    (local $d i32)
    (local $pos i32)
    local.get $pref
    i32.load
    local.set $plen
    local.get $suf
    i32.load
    local.set $slen
    ;; Digit count via magnitude compares (no `div` loop). ASCII digits = one scalar each.
    i32.const 0
    local.set $neg
    local.get $v
    i32.eqz
    if
        i32.const 1
        local.set $ndigits
    else
        local.get $v
        local.set $tmp
        local.get $v
        i32.const 0
        i32.lt_s
        if
            i32.const 1
            local.set $neg
            i32.const 0
            local.get $v
            i32.sub
            local.set $tmp
        end
        ;; INT_MIN has no positive i32 counterpart (`0 - min == min`).
        local.get $tmp
        i32.const 0
        i32.lt_s
        if
            i32.const 10
            local.set $ndigits
        else
            local.get $tmp
            i32.const 10
            i32.lt_u
            if
                i32.const 1
                local.set $ndigits
            else
                local.get $tmp
                i32.const 100
                i32.lt_u
                if
                    i32.const 2
                    local.set $ndigits
                else
                    local.get $tmp
                    i32.const 1000
                    i32.lt_u
                    if
                        i32.const 3
                        local.set $ndigits
                    else
                        local.get $tmp
                        i32.const 10000
                        i32.lt_u
                        if
                            i32.const 4
                            local.set $ndigits
                        else
                            local.get $tmp
                            i32.const 100000
                            i32.lt_u
                            if
                                i32.const 5
                                local.set $ndigits
                            else
                                local.get $tmp
                                i32.const 1000000
                                i32.lt_u
                                if
                                    i32.const 6
                                    local.set $ndigits
                                else
                                    local.get $tmp
                                    i32.const 10000000
                                    i32.lt_u
                                    if
                                        i32.const 7
                                        local.set $ndigits
                                    else
                                        local.get $tmp
                                        i32.const 100000000
                                        i32.lt_u
                                        if
                                            i32.const 8
                                            local.set $ndigits
                                        else
                                            local.get $tmp
                                            i32.const 1000000000
                                            i32.lt_u
                                            if
                                                i32.const 9
                                                local.set $ndigits
                                            else
                                                i32.const 10
                                                local.set $ndigits
                                            end
                                        end
                                    end
                                end
                            end
                        end
                    end
                end
            end
        end
        local.get $neg
        if
            local.get $ndigits
            i32.const 1
            i32.add
            local.set $ndigits
        end
    end
    local.get $plen
    local.get $ndigits
    i32.add
    local.get $slen
    i32.add
    local.set $total
    local.get $total
    i32.eqz
    if
        global.get $__rt_str_empty
        call $retain
        global.get $__rt_str_empty
        return
    end
    local.get $total
    i32.const 1
    i32.shl
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $p
    local.get $p
    local.get $total
    i32.store
    local.get $p
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    local.get $p
    i32.const 8
    i32.add
    local.set $d
    ;; copy prefix (UTF-16 bytes)
    local.get $plen
    i32.eqz
    i32.eqz
    if
        local.get $d
        local.get $pref
        i32.const 8
        i32.add
        local.get $plen
        i32.const 1
        i32.shl
        memory.copy
    end
    ;; write digits as UTF-16 units starting at unit index $plen
    local.get $d
    local.get $plen
    i32.const 1
    i32.shl
    i32.add
    local.set $pos
    local.get $v
    i32.eqz
    if
        local.get $pos
        i32.const 48
        i32.store16
    else
        local.get $neg
        if
            local.get $pos
            i32.const 45
            i32.store16
        end
        local.get $d
        local.get $plen
        local.get $ndigits
        i32.add
        i32.const 1
        i32.shl
        i32.add
        local.set $pos
        local.get $neg
        if
            i32.const 0
            local.get $v
            i32.sub
            local.set $tmp
        else
            local.get $v
            local.set $tmp
        end
        (block $write_done
            (loop $write
                local.get $tmp
                i32.eqz
                br_if $write_done
                local.get $pos
                i32.const 2
                i32.sub
                local.set $pos
                local.get $pos
                local.get $tmp
                i32.const 10
                i32.rem_u
                i32.const 48
                i32.add
                i32.store16
                local.get $tmp
                i32.const 10
                i32.div_u
                local.set $tmp
                br $write
            )
        )
    end
    ;; copy suffix
    local.get $slen
    i32.eqz
    i32.eqz
    if
        local.get $d
        local.get $plen
        local.get $ndigits
        i32.add
        i32.const 1
        i32.shl
        i32.add
        local.get $suf
        i32.const 8
        i32.add
        local.get $slen
        i32.const 1
        i32.shl
        memory.copy
    end
    local.get $p
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
    local.get $len
    i32.const 1
    i32.shl
    local.set $len
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

;; UTF-8 byte lexicographic compare (same order as Unicode scalars). Negative / zero / positive
;; as i32, matching `Comparable.compare`. Null is treated as empty.
(func $string_compare (param $a i32) (param $b i32) (result i32)
    (local $len_a i32)
    (local $len_b i32)
    (local $n i32)
    (local $i i32)
    (local $words i32)
    (local $wa i32)
    (local $wb i32)
    (local $ba i32)
    (local $bb i32)
    local.get $a
    local.get $b
    i32.eq
    if
        i32.const 0
        return
    end
    local.get $a
    i32.eqz
    if
        i32.const 0
        local.set $len_a
    else
        local.get $a
        i32.load
        local.set $len_a
    end
    local.get $b
    i32.eqz
    if
        i32.const 0
        local.set $len_b
    else
        local.get $b
        i32.load
        local.set $len_b
    end
    local.get $len_a
    i32.const 1
    i32.shl
    local.set $len_a
    local.get $len_b
    i32.const 1
    i32.shl
    local.set $len_b
    local.get $len_a
    local.get $len_b
    i32.lt_u
    if
        local.get $len_a
        local.set $n
    else
        local.get $len_b
        local.set $n
    end
    local.get $n
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
            i32.eqz
            if
                i32.const 0
                local.set $wa
            else
                local.get $a
                i32.const 8
                i32.add
                local.get $i
                i32.const 2
                i32.shl
                i32.add
                i32.load
                local.set $wa
            end
            local.get $b
            i32.eqz
            if
                i32.const 0
                local.set $wb
            else
                local.get $b
                i32.const 8
                i32.add
                local.get $i
                i32.const 2
                i32.shl
                i32.add
                i32.load
                local.set $wb
            end
            local.get $wa
            local.get $wb
            i32.ne
            if
                i32.const 0
                local.set $ba
                (loop $byte_in_word
                    local.get $wa
                    local.get $ba
                    i32.const 3
                    i32.shl
                    i32.shr_u
                    i32.const 255
                    i32.and
                    local.set $bb
                    local.get $wb
                    local.get $ba
                    i32.const 3
                    i32.shl
                    i32.shr_u
                    i32.const 255
                    i32.and
                    local.get $bb
                    i32.ne
                    if
                        local.get $bb
                        local.get $wb
                        local.get $ba
                        i32.const 3
                        i32.shl
                        i32.shr_u
                        i32.const 255
                        i32.and
                        i32.lt_u
                        if
                            i32.const -1
                            return
                        end
                        i32.const 1
                        return
                    end
                    local.get $ba
                    i32.const 1
                    i32.add
                    local.set $ba
                    local.get $ba
                    i32.const 4
                    i32.lt_u
                    br_if $byte_in_word
                )
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
            local.get $n
            i32.ge_u
            br_if $tail_done
            local.get $a
            i32.eqz
            if
                i32.const 0
                local.set $ba
            else
                local.get $a
                i32.const 8
                i32.add
                local.get $i
                i32.add
                i32.load8_u
                local.set $ba
            end
            local.get $b
            i32.eqz
            if
                i32.const 0
                local.set $bb
            else
                local.get $b
                i32.const 8
                i32.add
                local.get $i
                i32.add
                i32.load8_u
                local.set $bb
            end
            local.get $ba
            local.get $bb
            i32.ne
            if
                local.get $ba
                local.get $bb
                i32.lt_u
                if
                    i32.const -1
                    return
                end
                i32.const 1
                return
            end
            local.get $i
            i32.const 1
            i32.add
            local.set $i
            br $tail
        )
    )
    local.get $len_a
    local.get $len_b
    i32.lt_u
    if
        i32.const -1
        return
    end
    local.get $len_a
    local.get $len_b
    i32.gt_u
    if
        i32.const 1
        return
    end
    i32.const 0
)

;; Scalar-indexed substring: clamp `[start, end)`, map to UTF-8 byte offsets, then one
;; `malloc` + `memory.copy`. Empty / null yields the interned `$string.empty` pointer.
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
        global.get $__rt_str_empty
        return
    end
    local.get $ptr
    i32.load
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
    local.get $s
    i32.const 1
    i32.shl
    local.set $byte_start
    local.get $e
    local.get $s
    i32.sub
    local.set $scalars
    local.get $scalars
    i32.const 1
    i32.shl
    local.set $byte_len
    local.get $byte_len
    i32.eqz
    if
        global.get $__rt_str_empty
        return
    end
    local.get $byte_len
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $p
    local.get $p
    local.get $scalars
    i32.store
    local.get $p
    i32.const 4
    i32.add
    i32.const 0
    i32.store
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

;; Isolation-boundary copy: a new heap string with the same payload (empty stays interned).
(func $string_clone (param $ptr i32) (result i32)
    local.get $ptr
    i32.eqz
    if
        global.get $__rt_str_empty
        return
    end
    local.get $ptr
    i32.const 0
    local.get $ptr
    call $str_scalar_len
    call $string_substring_raw
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

;; Unchecked UTF-16 unit read; call sites emit a unit-index bounds check first.
(func $char_at (param $ptr i32) (param $i i32) (result i32)
    local.get $ptr
    i32.const 8
    i32.add
    local.get $i
    i32.const 1
    i32.shl
    i32.add
    i32.load16_u
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

;; Allocates an empty string buffer with room for up to `n` UTF-16 units.
(func $string_alloc (param $n i32) (result i32)
    (local $p i32)
    local.get $n
    i32.const 1
    i32.shl
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

;; Transcode raw UTF-8 `[base, byte_len)` into a new UTF-16 string.
(func $utf8_bytes_to_string (param $base i32) (param $byte_len i32) (result i32)
    (local $p i32)
    (local $off i32)
    (local $units i32)
    (local $dst i32)
    (local $cp i32)
    (local $w i32)
    local.get $byte_len
    i32.eqz
    if
        global.get $__rt_str_empty
        return
    end
    local.get $byte_len
    i32.const 1
    i32.shl
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $p
    local.get $p
    i32.const 8
    i32.add
    local.set $dst
    i32.const 0
    local.set $off
    i32.const 0
    local.set $units
    (block $done
        (loop $scan
            local.get $off
            local.get $byte_len
            i32.ge_u
            br_if $done
            local.get $base
            local.get $off
            call $utf8_decode_raw
            local.set $cp
            local.get $dst
            local.get $units
            local.get $cp
            call $utf16_encode_at
            local.set $w
            local.get $units
            local.get $w
            i32.add
            local.set $units
            local.get $off
            local.get $base
            local.get $off
            call $utf8_width_raw
            i32.add
            local.set $off
            br $scan
        )
    )
    local.get $p
    local.get $units
    i32.store
    local.get $p
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    local.get $p
)

;; Builds a string from a `byte[]` UTF-8 payload (`[count:i32][bytes...]`). Empty/null-safe.
(func $string_from_utf8 (param $bytes i32) (result i32)
    local.get $bytes
    i32.eqz
    if
        global.get $__rt_str_empty
        return
    end
    local.get $bytes
    i32.const 4
    i32.add
    local.get $bytes
    i32.load
    call $utf8_bytes_to_string
)

;; Writes code unit/scalar `c` at unit index `i`, appending when `i` equals the current unit count.
(func $string_set (param $ptr i32) (param $i i32) (param $c i32)
    (local $n i32)
    (local $w i32)
    local.get $ptr
    call $str_scalar_len
    local.set $n
    local.get $i
    local.get $n
    i32.gt_u
    if
        unreachable
    end
    local.get $c
    i32.const 0x10000
    i32.ge_u
    if
        local.get $i
        local.get $n
        i32.eq
        if
            local.get $ptr
            i32.const 8
            i32.add
            local.get $n
            local.get $c
            call $utf16_encode_at
            local.set $w
            local.get $ptr
            local.get $n
            local.get $w
            i32.add
            i32.store
            return
        end
        i32.const 0xFFFD
        local.set $c
    end
    local.get $ptr
    i32.const 8
    i32.add
    local.get $i
    i32.const 1
    i32.shl
    i32.add
    local.get $c
    i32.store16
    local.get $i
    local.get $n
    i32.eq
    if
        local.get $ptr
        local.get $n
        i32.const 1
        i32.add
        i32.store
    end
)

(func $string_from_utf8_prefix (param $bytes i32) (param $len i32) (result i32)
    (local $count i32)
    local.get $bytes
    i32.eqz
    if
        global.get $__rt_str_empty
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
    local.get $bytes
    i32.const 4
    i32.add
    local.get $len
    call $utf8_bytes_to_string
)

;; Copy `len` payload bytes from a `byte[]` as UTF-16 LE; `scalars` is the unit count.
(func $string_from_utf8_prefix_n (param $bytes i32) (param $len i32) (param $scalars i32) (result i32)
    (local $count i32)
    (local $p i32)
    local.get $bytes
    i32.eqz
    if
        global.get $__rt_str_empty
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
        global.get $__rt_str_empty
        return
    end
    local.get $scalars
    i32.const 0
    i32.lt_s
    if
        local.get $len
        i32.const 1
        i32.shr_u
        local.set $scalars
    end
    local.get $len
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $p
    local.get $p
    local.get $scalars
    i32.store
    local.get $p
    i32.const 4
    i32.add
    i32.const 0
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

;; `i32.store16` into a `byte[]` payload at byte offset `off`.
(func $array_store16 (param $arr i32) (param $off i32) (param $u i32)
    local.get $arr
    i32.eqz
    br_if 0
    local.get $arr
    i32.const 4
    i32.add
    local.get $off
    i32.add
    local.get $u
    i32.store16
)

;; Finish a StringBuilder whose `byte[]` reserves 4 pad bytes at payload start (`[len][pad][utf16]`).
;; Unique buffers are retagged in place; otherwise this copies from offset 8.
(func $string_from_builder (param $bytes i32) (param $len i32) (param $scalars i32) (result i32)
    (local $p i32)
    local.get $bytes
    i32.eqz
    if
        global.get $__rt_str_empty
        return
    end
    local.get $len
    i32.const 0
    i32.le_s
    if
        global.get $__rt_str_empty
        return
    end
    local.get $scalars
    i32.const 0
    i32.lt_s
    if
        local.get $len
        i32.const 1
        i32.shr_u
        local.set $scalars
    end
    local.get $len
    i32.const 8
    i32.add
    i32.const {TAG_STRING}
    call $malloc
    local.set $p
    local.get $p
    local.get $scalars
    i32.store
    local.get $p
    i32.const 4
    i32.add
    i32.const 0
    i32.store
    local.get $p
    i32.const 8
    i32.add
    local.get $bytes
    i32.const 8
    i32.add
    local.get $len
    memory.copy
    local.get $p
)

