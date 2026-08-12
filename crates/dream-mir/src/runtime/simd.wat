;; SIMD helpers for `system.simd` / AOT autovec. Addresses are `T[]` data pointers (length at [0],
;; elements at +4). Offsets are element indices.
(func $simd_f32x4_add (param $dest i32) (param $doff i32) (param $a i32) (param $aoff i32) (param $b i32) (param $boff i32)
    local.get $dest
    i32.const 4
    i32.add
    local.get $doff
    i32.const 2
    i32.shl
    i32.add
    local.get $a
    i32.const 4
    i32.add
    local.get $aoff
    i32.const 2
    i32.shl
    i32.add
    v128.load
    local.get $b
    i32.const 4
    i32.add
    local.get $boff
    i32.const 2
    i32.shl
    i32.add
    v128.load
    f32x4.add
    v128.store
)
(func $simd_f32x4_sub (param $dest i32) (param $doff i32) (param $a i32) (param $aoff i32) (param $b i32) (param $boff i32)
    local.get $dest
    i32.const 4
    i32.add
    local.get $doff
    i32.const 2
    i32.shl
    i32.add
    local.get $a
    i32.const 4
    i32.add
    local.get $aoff
    i32.const 2
    i32.shl
    i32.add
    v128.load
    local.get $b
    i32.const 4
    i32.add
    local.get $boff
    i32.const 2
    i32.shl
    i32.add
    v128.load
    f32x4.sub
    v128.store
)
(func $simd_f32x4_mul (param $dest i32) (param $doff i32) (param $a i32) (param $aoff i32) (param $b i32) (param $boff i32)
    local.get $dest
    i32.const 4
    i32.add
    local.get $doff
    i32.const 2
    i32.shl
    i32.add
    local.get $a
    i32.const 4
    i32.add
    local.get $aoff
    i32.const 2
    i32.shl
    i32.add
    v128.load
    local.get $b
    i32.const 4
    i32.add
    local.get $boff
    i32.const 2
    i32.shl
    i32.add
    v128.load
    f32x4.mul
    v128.store
)
(func $simd_i32x4_add (param $dest i32) (param $doff i32) (param $a i32) (param $aoff i32) (param $b i32) (param $boff i32)
    local.get $dest
    i32.const 4
    i32.add
    local.get $doff
    i32.const 2
    i32.shl
    i32.add
    local.get $a
    i32.const 4
    i32.add
    local.get $aoff
    i32.const 2
    i32.shl
    i32.add
    v128.load
    local.get $b
    i32.const 4
    i32.add
    local.get $boff
    i32.const 2
    i32.shl
    i32.add
    v128.load
    i32x4.add
    v128.store
)
