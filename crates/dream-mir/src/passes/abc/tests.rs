use super::*;
use crate::build::FunctionBuilder;
use crate::BinOp;

#[test]
fn foreach_shape_is_unchecked() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_param(arr_ty, Some("a".into()));
    let idx = b.new_temp(i.int());
    let len = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    b.assign(
        Place::Local(idx),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    b.assign(
        Place::Local(len),
        Rvalue::ArrayLen(Operand::Copy(Place::Local(arr))),
    );
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(idx)),
            Operand::Copy(Place::Local(len)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(cmp)),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(elem),
        Rvalue::Use(Operand::Copy(Place::index(
            arr,
            Operand::Copy(Place::Local(idx)),
        ))),
    );
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(idx)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(elem)))));
    let mut func = b.finish();
    assert!(Abc.run(&mut func, &i));
    match &func.blocks[body.0 as usize].stmts[0] {
        Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked, .. }))) => {
            assert!(*unchecked);
        }
        other => panic!("expected unchecked index, got {:?}", other),
    }
}

#[test]
fn alloc_len_bound_is_unchecked() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.float());
    let mut b = FunctionBuilder::new("f", i.int());
    let n = b.new_param(i.int(), Some("n".into()));
    let arr = b.new_temp(arr_ty);
    let idx = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.float());
    b.assign(
        Place::Local(arr),
        Rvalue::ArrayNew {
            elem_ty: i.float(),
            len: Operand::Copy(Place::Local(n)),
        },
    );
    b.assign(
        Place::Local(idx),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(idx)),
            Operand::Copy(Place::Local(n)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(cmp)),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(elem),
        Rvalue::Use(Operand::Copy(Place::index(
            arr,
            Operand::Copy(Place::Local(idx)),
        ))),
    );
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(idx)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(n)))));
    let mut func = b.finish();
    assert!(Abc.run(&mut func, &i));
    match &func.blocks[body.0 as usize].stmts[0] {
        Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked, .. }))) => {
            assert!(*unchecked);
        }
        other => panic!("expected unchecked index, got {:?}", other),
    }
}

/// `release a; a = null` after the loop (RC insertion's last-use null-out) is not a second
/// definition that could shrink `a`, so the constant allocation length still bounds `i < 64`.
#[test]
fn const_alloc_len_survives_release_null_out() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.float());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_temp(arr_ty);
    let idx = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.float());
    b.assign(
        Place::Local(arr),
        Rvalue::ArrayNew {
            elem_ty: i.float(),
            len: Operand::Const(Const::Int(64)),
        },
    );
    b.assign(
        Place::Local(idx),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(idx)),
            Operand::Const(Const::Int(64)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(cmp)),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(elem),
        Rvalue::Use(Operand::Copy(Place::index(
            arr,
            Operand::Copy(Place::Local(idx)),
        ))),
    );
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(idx)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.push(Statement::Release(Operand::Copy(Place::Local(arr))));
    b.assign(Place::Local(arr), Rvalue::Use(Operand::Const(Const::Null)));
    b.terminate(Terminator::Return(Some(Operand::Const(Const::Int(0)))));
    let mut func = b.finish();
    let blocks_before = func.blocks.len();
    assert!(Abc.run(&mut func, &i));
    assert_eq!(func.blocks.len(), blocks_before, "proven loop must not be versioned");
    match &func.blocks[body.0 as usize].stmts[0] {
        Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked, .. }))) => {
            assert!(*unchecked);
        }
        other => panic!("expected unchecked index, got {:?}", other),
    }
}

#[test]
fn char_at_scan_shape_is_unchecked() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.int());
    let s = b.new_param(i.string(), Some("s".into()));
    let idx = b.new_temp(i.int());
    let len = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let ch = b.new_temp(i.char());
    b.assign(
        Place::Local(idx),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    b.assign(
        Place::Local(len),
        Rvalue::StrLen(Operand::Copy(Place::Local(s))),
    );
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(idx)),
            Operand::Copy(Place::Local(len)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(cmp)),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(ch),
        Rvalue::CharAt(
            Operand::Copy(Place::Local(s)),
            Operand::Copy(Place::Local(idx)),
            false,
        ),
    );
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(idx)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(ch)))));
    let mut func = b.finish();
    assert!(Abc.run(&mut func, &i));
    match &func.blocks[body.0 as usize].stmts[0] {
        Statement::Assign(_, Rvalue::CharAt(_, _, true)) => {}
        other => panic!("expected unchecked char_at, got {:?}", other),
    }
}

#[test]
fn interned_string_scan_is_unchecked() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.int());
    let idx = b.new_temp(i.int());
    let len = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let ch = b.new_temp(i.char());
    let lit = Operand::Const(Const::Str("abc".into()));
    b.assign(
        Place::Local(idx),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    b.assign(Place::Local(len), Rvalue::StrLen(lit.clone()));
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(idx)),
            Operand::Copy(Place::Local(len)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(cmp)),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(ch),
        Rvalue::CharAt(lit, Operand::Copy(Place::Local(idx)), false),
    );
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(idx)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(ch)))));
    let mut func = b.finish();
    assert!(Abc.run(&mut func, &i));
    match &func.blocks[body.0 as usize].stmts[0] {
        Statement::Assign(_, Rvalue::CharAt(_, _, true)) => {}
        other => panic!(
            "expected unchecked char_at on interned string, got {:?}",
            other
        ),
    }
}

#[test]
fn affine_index_is_unchecked() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.float());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_temp(arr_ty);
    let n = b.new_temp(i.int());
    let iv = b.new_temp(i.int());
    let j = b.new_temp(i.int());
    let mul = b.new_temp(i.int());
    let idx = b.new_temp(i.int());
    let ci = b.new_temp(i.bool());
    let cj = b.new_temp(i.bool());
    let elem = b.new_temp(i.float());
    b.assign(
        Place::Local(n),
        Rvalue::Use(Operand::Const(Const::Int(64))),
    );
    b.assign(
        Place::Local(arr),
        Rvalue::ArrayNew {
            elem_ty: i.float(),
            len: Operand::Const(Const::Int(4096)),
        },
    );
    b.assign(
        Place::Local(iv),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    let icond = b.new_block();
    let ibody = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(icond));
    b.switch_to(icond);
    b.assign(
        Place::Local(ci),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(iv)),
            Operand::Copy(Place::Local(n)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(ci)),
        then_blk: ibody,
        else_blk: after,
    });
    b.switch_to(ibody);
    b.assign(
        Place::Local(j),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    let jcond = b.new_block();
    let jbody = b.new_block();
    let ilatch = b.new_block();
    b.terminate(Terminator::Goto(jcond));
    b.switch_to(jcond);
    b.assign(
        Place::Local(cj),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(j)),
            Operand::Const(Const::Int(64)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(cj)),
        then_blk: jbody,
        else_blk: ilatch,
    });
    b.switch_to(jbody);
    b.assign(
        Place::Local(mul),
        Rvalue::Binary(
            BinOp::Mul,
            Operand::Copy(Place::Local(iv)),
            Operand::Copy(Place::Local(n)),
        ),
    );
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(mul)),
            Operand::Copy(Place::Local(j)),
        ),
    );
    b.assign(
        Place::Local(elem),
        Rvalue::Use(Operand::Copy(Place::index(
            arr,
            Operand::Copy(Place::Local(idx)),
        ))),
    );
    b.assign(
        Place::Local(j),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(j)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(jcond));
    b.switch_to(ilatch);
    b.assign(
        Place::Local(iv),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(iv)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(icond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(elem)))));
    let mut func = b.finish();
    assert!(Abc.run(&mut func, &i));
    let unchecked = func.blocks[jbody.0 as usize].stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked: true, .. })))
        )
    });
    assert!(unchecked, "i * n + j must drop the bounds check");
}

/// `idx = (i << 6) + k` in a loop whose `k = k + 1` lives in a latch the body dominates.
/// The latch stores the next iteration; it must not hide the `k < 64` guard.
#[test]
fn shift_affine_index_with_latch_is_unchecked() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.float());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_temp(arr_ty);
    let iv = b.new_temp(i.int());
    let k = b.new_temp(i.int());
    let scale = b.new_temp(i.int());
    let idx = b.new_temp(i.int());
    let ci = b.new_temp(i.bool());
    let ck = b.new_temp(i.bool());
    let elem = b.new_temp(i.float());
    b.assign(
        Place::Local(arr),
        Rvalue::ArrayNew {
            elem_ty: i.float(),
            len: Operand::Const(Const::Int(4096)),
        },
    );
    b.assign(
        Place::Local(iv),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    let icond = b.new_block();
    let ibody = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(icond));
    b.switch_to(icond);
    b.assign(
        Place::Local(ci),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(iv)),
            Operand::Const(Const::Int(64)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(ci)),
        then_blk: ibody,
        else_blk: after,
    });
    b.switch_to(ibody);
    b.assign(
        Place::Local(scale),
        Rvalue::Binary(
            BinOp::Shl,
            Operand::Copy(Place::Local(iv)),
            Operand::Const(Const::Int(6)),
        ),
    );
    b.assign(
        Place::Local(k),
        Rvalue::Use(Operand::Const(Const::Int(0))),
    );
    let kcond = b.new_block();
    let kbody = b.new_block();
    let klatch = b.new_block();
    let ilatch = b.new_block();
    b.terminate(Terminator::Goto(kcond));
    b.switch_to(kcond);
    b.assign(
        Place::Local(ck),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(k)),
            Operand::Const(Const::Int(64)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(ck)),
        then_blk: kbody,
        else_blk: ilatch,
    });
    b.switch_to(kbody);
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(scale)),
            Operand::Copy(Place::Local(k)),
        ),
    );
    b.assign(
        Place::Local(elem),
        Rvalue::Use(Operand::Copy(Place::index(
            arr,
            Operand::Copy(Place::Local(idx)),
        ))),
    );
    b.terminate(Terminator::Goto(klatch));
    b.switch_to(klatch);
    b.assign(
        Place::Local(k),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(k)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(kcond));
    b.switch_to(ilatch);
    b.assign(
        Place::Local(iv),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(iv)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(icond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(elem)))));
    let mut func = b.finish();
    assert!(Abc.run(&mut func, &i));
    let unchecked = func.blocks[kbody.0 as usize].stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked: true, .. })))
        )
    });
    assert!(unchecked, "i << 6 + k must drop the bounds check");
}

#[test]
fn square_bound_index_is_unchecked() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_temp(arr_ty);
    let iv = b.new_temp(i.int());
    let sq = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    b.assign(
        Place::Local(arr),
        Rvalue::ArrayNew {
            elem_ty: i.int(),
            len: Operand::Const(Const::Int(4096)),
        },
    );
    b.assign(
        Place::Local(iv),
        Rvalue::Use(Operand::Const(Const::Int(2))),
    );
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(sq),
        Rvalue::Binary(
            BinOp::Mul,
            Operand::Copy(Place::Local(iv)),
            Operand::Copy(Place::Local(iv)),
        ),
    );
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(sq)),
            Operand::Const(Const::Int(4096)),
        ),
    );
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(cmp)),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    b.assign(
        Place::Local(elem),
        Rvalue::Use(Operand::Copy(Place::index(
            arr,
            Operand::Copy(Place::Local(iv)),
        ))),
    );
    b.assign(
        Place::Local(iv),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(iv)),
            Operand::Const(Const::Int(1)),
        ),
    );
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(elem)))));
    let mut func = b.finish();
    assert!(Abc.run(&mut func, &i));
    let unchecked = func.blocks[body.0 as usize].stmts.iter().any(|s| {
        matches!(
            s,
            Statement::Assign(_, Rvalue::Use(Operand::Copy(Place::Index { unchecked: true, .. })))
        )
    });
    assert!(unchecked, "i under i * i < len must drop the bounds check");
}

fn copy(l: Local) -> Operand {
    Operand::Copy(Place::Local(l))
}

fn int(v: i64) -> Operand {
    Operand::Const(Const::Int(v))
}

fn load(b: &mut FunctionBuilder, dst: Local, arr: Local, idx: Local) {
    b.assign(
        Place::Local(dst),
        Rvalue::Use(Operand::Copy(Place::index(arr, copy(idx)))),
    );
}

fn index_flags(func: &MirFunction, block: BlockId) -> Vec<bool> {
    let mut out = Vec::new();
    for stmt in &func.blocks[block.0 as usize].stmts {
        visit_stmt_accesses(stmt, &mut |a| {
            if let Access::Index { unchecked, .. } = a {
                out.push(unchecked);
            }
        });
    }
    out
}

fn all_index_flags(func: &MirFunction) -> Vec<bool> {
    (0..func.blocks.len())
        .flat_map(|b| index_flags(func, BlockId(b as u32)))
        .collect()
}

/// `i = 0; while i < a.length { <body_extra>; x = a[i]; i = i + 1 }` then `after`.
struct CountedLoop {
    func: FunctionBuilder,
    arr: Local,
    idx: Local,
    elem: Local,
    body: BlockId,
    after: BlockId,
}

fn counted_loop(i: &mut TypeInterner) -> CountedLoop {
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_param(arr_ty, Some("a".into()));
    let idx = b.new_temp(i.int());
    let len = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    b.assign(Place::Local(idx), Rvalue::Use(int(0)));
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(Place::Local(len), Rvalue::ArrayLen(copy(arr)));
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(BinOp::Lt, copy(idx), copy(len)),
    );
    b.terminate(Terminator::If {
        cond: copy(cmp),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    CountedLoop {
        func: b,
        arr,
        idx,
        elem,
        body,
        after,
    }
}

fn increment(b: &mut FunctionBuilder, idx: Local) {
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(BinOp::Add, copy(idx), int(1)),
    );
}

#[test]
fn access_after_loop_exit_stays_checked() {
    let mut i = TypeInterner::new();
    let mut l = counted_loop(&mut i);
    load(&mut l.func, l.elem, l.arr, l.idx);
    increment(&mut l.func, l.idx);
    let cond = BlockId(1);
    l.func.terminate(Terminator::Goto(cond));
    l.func.switch_to(l.after);
    load(&mut l.func, l.elem, l.arr, l.idx);
    l.func.terminate(Terminator::Return(Some(copy(l.elem))));
    let mut func = l.func.finish();
    Abc.run(&mut func, &i);
    assert_eq!(index_flags(&func, l.body), vec![true]);
    assert_eq!(index_flags(&func, l.after), vec![false]);
}

#[test]
fn increment_before_access_in_same_block_stays_checked() {
    let mut i = TypeInterner::new();
    let mut l = counted_loop(&mut i);
    increment(&mut l.func, l.idx);
    load(&mut l.func, l.elem, l.arr, l.idx);
    l.func.terminate(Terminator::Goto(BlockId(1)));
    l.func.switch_to(l.after);
    l.func.terminate(Terminator::Return(Some(copy(l.elem))));
    let mut func = l.func.finish();
    Abc.run(&mut func, &i);
    assert_eq!(index_flags(&func, l.body), vec![false]);
}

#[test]
fn increment_on_one_branch_keeps_join_access_checked() {
    let mut i = TypeInterner::new();
    let mut l = counted_loop(&mut i);
    let flag = l.func.new_param(i.bool(), Some("flag".into()));
    let bump = l.func.new_block();
    let join = l.func.new_block();
    l.func.terminate(Terminator::If {
        cond: copy(flag),
        then_blk: bump,
        else_blk: join,
    });
    l.func.switch_to(bump);
    increment(&mut l.func, l.idx);
    l.func.terminate(Terminator::Goto(join));
    l.func.switch_to(join);
    load(&mut l.func, l.elem, l.arr, l.idx);
    increment(&mut l.func, l.idx);
    l.func.terminate(Terminator::Goto(BlockId(1)));
    l.func.switch_to(l.after);
    l.func.terminate(Terminator::Return(Some(copy(l.elem))));
    let mut func = l.func.finish();
    Abc.run(&mut func, &i);
    assert_eq!(index_flags(&func, join), vec![false]);
}

#[test]
fn a_negative_definition_defeats_nonneg() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_param(arr_ty, Some("a".into()));
    let idx = b.new_temp(i.int());
    let len = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    b.assign(Place::Local(idx), Rvalue::Use(int(0)));
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(BinOp::Sub, copy(idx), int(1)),
    );
    b.assign(Place::Local(len), Rvalue::ArrayLen(copy(arr)));
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(BinOp::Lt, copy(idx), copy(len)),
    );
    let then = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::If {
        cond: copy(cmp),
        then_blk: then,
        else_blk: after,
    });
    b.switch_to(then);
    load(&mut b, elem, arr, idx);
    b.terminate(Terminator::Goto(after));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(copy(elem))));
    let mut func = b.finish();
    Abc.run(&mut func, &i);
    assert_eq!(index_flags(&func, then), vec![false]);
}

#[test]
fn wrapping_increment_without_a_guard_defeats_nonneg() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_param(arr_ty, Some("a".into()));
    let idx = b.new_temp(i.int());
    let len = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    b.assign(Place::Local(idx), Rvalue::Use(int(i32::MAX as i64)));
    increment(&mut b, idx);
    b.assign(Place::Local(len), Rvalue::ArrayLen(copy(arr)));
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(BinOp::Lt, copy(idx), copy(len)),
    );
    let then = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::If {
        cond: copy(cmp),
        then_blk: then,
        else_blk: after,
    });
    b.switch_to(then);
    load(&mut b, elem, arr, idx);
    b.terminate(Terminator::Goto(after));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(copy(elem))));
    let mut func = b.finish();
    Abc.run(&mut func, &i);
    assert_eq!(index_flags(&func, then), vec![false]);
}

/// `n = a.length; i = n - 1; while i >= 0 { x = a[i]; i = i - 1 }`.
#[test]
fn decreasing_loop_is_unchecked() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_param(arr_ty, Some("a".into()));
    let n = b.new_temp(i.int());
    let idx = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    b.assign(Place::Local(n), Rvalue::ArrayLen(copy(arr)));
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(BinOp::Sub, copy(n), int(1)),
    );
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(BinOp::Ge, copy(idx), int(0)),
    );
    b.terminate(Terminator::If {
        cond: copy(cmp),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    load(&mut b, elem, arr, idx);
    b.assign(
        Place::Local(idx),
        Rvalue::Binary(BinOp::Sub, copy(idx), int(1)),
    );
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(copy(elem))));
    let mut func = b.finish();
    assert!(Abc.run(&mut func, &i));
    assert_eq!(index_flags(&func, body), vec![true]);
}

/// `last = a.length - 1; i = 0; while i <= last { x = a[i]; i = i + 1 }`.
#[test]
fn inclusive_bound_is_unchecked() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_param(arr_ty, Some("a".into()));
    let n = b.new_temp(i.int());
    let last = b.new_temp(i.int());
    let idx = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    b.assign(Place::Local(n), Rvalue::ArrayLen(copy(arr)));
    b.assign(
        Place::Local(last),
        Rvalue::Binary(BinOp::Sub, copy(n), int(1)),
    );
    b.assign(Place::Local(idx), Rvalue::Use(int(0)));
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(BinOp::Le, copy(idx), copy(last)),
    );
    b.terminate(Terminator::If {
        cond: copy(cmp),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    load(&mut b, elem, arr, idx);
    b.terminate(Terminator::Goto(BlockId(4)));
    let latch = b.new_block();
    b.switch_to(latch);
    increment(&mut b, idx);
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(copy(elem))));
    let mut func = b.finish();
    assert!(Abc.run(&mut func, &i));
    assert_eq!(index_flags(&func, body), vec![true]);
}

/// The `List` indexer shape: `if i < 0 { panic } if i >= a.length { panic } x = a[i]`.
#[test]
fn rejecting_guards_on_else_edges_are_unchecked() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_param(arr_ty, Some("a".into()));
    let idx = b.new_param(i.int(), Some("i".into()));
    let neg = b.new_temp(i.bool());
    let len = b.new_temp(i.int());
    let over = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    b.assign(
        Place::Local(neg),
        Rvalue::Binary(BinOp::Lt, copy(idx), int(0)),
    );
    let fail = b.new_block();
    let second = b.new_block();
    let ok = b.new_block();
    b.terminate(Terminator::If {
        cond: copy(neg),
        then_blk: fail,
        else_blk: second,
    });
    b.switch_to(fail);
    b.terminate(Terminator::Unreachable);
    b.switch_to(second);
    b.assign(Place::Local(len), Rvalue::ArrayLen(copy(arr)));
    b.assign(
        Place::Local(over),
        Rvalue::Binary(BinOp::Ge, copy(idx), copy(len)),
    );
    let fail2 = b.new_block();
    b.terminate(Terminator::If {
        cond: copy(over),
        then_blk: fail2,
        else_blk: ok,
    });
    b.switch_to(fail2);
    b.terminate(Terminator::Unreachable);
    b.switch_to(ok);
    load(&mut b, elem, arr, idx);
    b.terminate(Terminator::Return(Some(copy(elem))));
    let mut func = b.finish();
    assert!(Abc.run(&mut func, &i));
    assert_eq!(index_flags(&func, ok), vec![true]);
}

/// `i = 0; while i < n { x = a[i]; i = i + 1 }` with `n` unrelated to `a.length`: the loop is
/// cloned behind `n <= a.length && i >= 0`, and only the clone drops the check.
#[test]
fn loop_with_foreign_bound_is_versioned() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_param(arr_ty, Some("a".into()));
    let n = b.new_param(i.int(), Some("n".into()));
    let idx = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    b.assign(Place::Local(idx), Rvalue::Use(int(0)));
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(BinOp::Lt, copy(idx), copy(n)),
    );
    b.terminate(Terminator::If {
        cond: copy(cmp),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    load(&mut b, elem, arr, idx);
    increment(&mut b, idx);
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(copy(elem))));
    let mut func = b.finish();
    let before = func.blocks.len();
    assert!(Abc.run(&mut func, &i));
    assert_eq!(func.blocks.len(), before + 2 + 2, "clone of cond+body plus two guards");
    assert_eq!(index_flags(&func, body), vec![false]);
    let mut flags = all_index_flags(&func);
    flags.sort();
    assert_eq!(flags, vec![false, true]);
    let entry_goes_to_guard = matches!(
        func.blocks[0].terminator,
        Terminator::Goto(t) if t.0 as usize >= before + 2
    );
    assert!(entry_goes_to_guard);
    // Idempotent: neither the original nor the clone qualifies again.
    let blocks = func.blocks.len();
    Abc.run(&mut func, &i);
    assert_eq!(func.blocks.len(), blocks);
}

fn foreign_bound_loop(null_before: bool, null_after: bool) -> (MirFunction, TypeInterner) {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_param(arr_ty, Some("a".into()));
    let n = b.new_param(i.int(), Some("n".into()));
    let idx = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    let null_arr = |b: &mut FunctionBuilder| {
        b.assign(Place::Local(arr), Rvalue::Use(Operand::Const(Const::Null)));
    };
    if null_before {
        null_arr(&mut b);
    }
    b.assign(Place::Local(idx), Rvalue::Use(int(0)));
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(BinOp::Lt, copy(idx), copy(n)),
    );
    b.terminate(Terminator::If {
        cond: copy(cmp),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    load(&mut b, elem, arr, idx);
    increment(&mut b, idx);
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    if null_after {
        null_arr(&mut b);
    }
    b.terminate(Terminator::Return(Some(copy(elem))));
    (b.finish(), i)
}

#[test]
fn released_take_param_is_still_versioned() {
    let (mut func, i) = foreign_bound_loop(false, true);
    assert!(Abc.run(&mut func, &i));
    let mut flags = all_index_flags(&func);
    flags.sort();
    assert_eq!(flags, vec![false, true]);
}

#[test]
fn nulled_array_is_not_versioned() {
    let (mut func, i) = foreign_bound_loop(true, false);
    Abc.run(&mut func, &i);
    assert_eq!(all_index_flags(&func), vec![false]);
}

#[test]
fn versioning_skips_accesses_after_the_increment() {
    let mut i = TypeInterner::new();
    let arr_ty = i.array(i.int());
    let mut b = FunctionBuilder::new("f", i.int());
    let arr = b.new_param(arr_ty, Some("a".into()));
    let n = b.new_param(i.int(), Some("n".into()));
    let idx = b.new_temp(i.int());
    let cmp = b.new_temp(i.bool());
    let elem = b.new_temp(i.int());
    b.assign(Place::Local(idx), Rvalue::Use(int(0)));
    let cond = b.new_block();
    let body = b.new_block();
    let after = b.new_block();
    b.terminate(Terminator::Goto(cond));
    b.switch_to(cond);
    b.assign(
        Place::Local(cmp),
        Rvalue::Binary(BinOp::Lt, copy(idx), copy(n)),
    );
    b.terminate(Terminator::If {
        cond: copy(cmp),
        then_blk: body,
        else_blk: after,
    });
    b.switch_to(body);
    increment(&mut b, idx);
    load(&mut b, elem, arr, idx);
    b.terminate(Terminator::Goto(cond));
    b.switch_to(after);
    b.terminate(Terminator::Return(Some(copy(elem))));
    let mut func = b.finish();
    let before = func.blocks.len();
    Abc.run(&mut func, &i);
    assert_eq!(func.blocks.len(), before);
    assert_eq!(all_index_flags(&func), vec![false]);
}
