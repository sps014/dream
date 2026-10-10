use super::pipeline::RcElision;
use crate::Statement;
use crate::build::FunctionBuilder;
use crate::passes::MirPass;
use crate::passes::rc::RcInsertion;
use crate::{Local, Operand, Place, Rvalue, Terminator};
use dream_types::TypeInterner;

#[test]
fn elides_adjacent_retain_release() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    b.push(Statement::Retain(Operand::Copy(Place::Local(Local(0)))));
    b.push(Statement::Release(Operand::Copy(Place::Local(Local(0)))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcElision.run(&mut func, &i));
    assert!(func.blocks[0].stmts.is_empty());
}

#[test]
fn elides_retain_release_separated_by_pure_arithmetic() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let x = b.new_local(i.string(), Some("x".into()));
    let tmp = b.new_local(i.int(), Some("tmp".into()));
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.push(Statement::Assign(
        Place::Local(tmp),
        Rvalue::Binary(
            crate::BinOp::Add,
            Operand::Const(crate::Const::Int(1)),
            Operand::Const(crate::Const::Int(2)),
        ),
    ));
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcElision.run(&mut func, &i));
    assert_eq!(func.blocks[0].stmts.len(), 1);
    assert!(matches!(func.blocks[0].stmts[0], Statement::Assign(..)));
}

#[test]
fn does_not_elide_across_a_call() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let x = b.new_local(i.string(), Some("x".into()));
    let callee = crate::Callee {
        def: dream_types::DefId::root(0),
        args: vec![],
        ret: i.void(),
        take_params: vec![],
    };
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.push(Statement::Call {
        callee,
        args: vec![],
    });
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(!RcElision.run(&mut func, &i));
    assert_eq!(func.blocks[0].stmts.len(), 3);
}

#[test]
fn elides_across_a_pure_copy_to_a_different_local() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let x = b.new_local(i.string(), Some("x".into()));
    let y = b.new_local(i.int(), Some("y".into()));
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.push(Statement::Assign(
        Place::Local(y),
        Rvalue::Use(Operand::Copy(Place::Local(x))),
    ));
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcElision.run(&mut func, &i));
    assert_eq!(func.blocks[0].stmts.len(), 1);
}

#[test]
fn nested_retains_cancel_innermost_first() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let x = Local(0);
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcElision.run(&mut func, &i));
    assert!(func.blocks[0].stmts.is_empty());
}

#[test]
fn elides_across_goto_chain() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let x = b.new_local(i.string(), Some("x".into()));
    let mid = b.new_block();
    let end = b.new_block();
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Goto(mid));
    b.switch_to(mid);
    b.terminate(Terminator::Goto(end));
    b.switch_to(end);
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcElision.run(&mut func, &i));
    assert!(func.blocks.iter().all(|bb| bb.stmts.is_empty()));
}

#[test]
fn elides_across_transparent_diamond() {
    // Retain(x); if c { pure } else { pure }; Release(x)
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let x = b.new_local(i.string(), Some("x".into()));
    let tmp = b.new_local(i.int(), Some("tmp".into()));
    let then_blk = b.new_block();
    let else_blk = b.new_block();
    let join = b.new_block();
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::If {
        cond: Operand::Const(crate::Const::Bool(true)),
        then_blk,
        else_blk,
    });
    b.switch_to(then_blk);
    b.push(Statement::Assign(
        Place::Local(tmp),
        Rvalue::Use(Operand::Const(crate::Const::Int(1))),
    ));
    b.terminate(Terminator::Goto(join));
    b.switch_to(else_blk);
    b.push(Statement::Assign(
        Place::Local(tmp),
        Rvalue::Use(Operand::Const(crate::Const::Int(2))),
    ));
    b.terminate(Terminator::Goto(join));
    b.switch_to(join);
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcElision.run(&mut func, &i));
    let retains = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    let releases = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| matches!(s, Statement::Release(_)))
        .count();
    assert_eq!(retains, 0);
    assert_eq!(releases, 0);
}

#[test]
fn does_not_elide_diamond_with_call_in_arm() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let x = b.new_local(i.string(), Some("x".into()));
    let then_blk = b.new_block();
    let else_blk = b.new_block();
    let join = b.new_block();
    let callee = crate::Callee {
        def: dream_types::DefId::root(0),
        args: vec![],
        ret: i.void(),
        take_params: vec![],
    };
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::If {
        cond: Operand::Const(crate::Const::Bool(true)),
        then_blk,
        else_blk,
    });
    b.switch_to(then_blk);
    b.push(Statement::Call {
        callee,
        args: vec![],
    });
    b.terminate(Terminator::Goto(join));
    b.switch_to(else_blk);
    b.terminate(Terminator::Goto(join));
    b.switch_to(join);
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(!RcElision.run(&mut func, &i));
}

#[test]
fn elides_around_transparent_loop() {
    // entry: Retain(x); goto header
    // header: if c -> body else exit
    // body: pure; goto header
    // exit: Release(x)
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let x = b.new_local(i.string(), Some("x".into()));
    let tmp = b.new_local(i.int(), Some("tmp".into()));
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Const(crate::Const::Bool(true)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.push(Statement::Assign(
        Place::Local(tmp),
        Rvalue::Binary(
            crate::BinOp::Add,
            Operand::Const(crate::Const::Int(1)),
            Operand::Const(crate::Const::Int(1)),
        ),
    ));
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcElision.run(&mut func, &i));
    let retains = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    let releases = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| matches!(s, Statement::Release(_)))
        .count();
    assert_eq!(retains, 0);
    assert_eq!(releases, 0);
}

#[test]
fn elides_across_transparent_switch_via_postdom() {
    // Retain; switch to three transparent arms; join Release — not a simple If diamond.
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let x = b.new_local(i.string(), Some("x".into()));
    let a = b.new_block();
    let c = b.new_block();
    let d = b.new_block();
    let join = b.new_block();
    b.push(Statement::Retain(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Switch {
        value: Operand::Const(crate::Const::Int(0)),
        targets: vec![(0, a), (1, c)],
        default: d,
    });
    b.switch_to(a);
    b.terminate(Terminator::Goto(join));
    b.switch_to(c);
    b.terminate(Terminator::Goto(join));
    b.switch_to(d);
    b.terminate(Terminator::Goto(join));
    b.switch_to(join);
    b.push(Statement::Release(Operand::Copy(Place::Local(x))));
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    assert!(RcElision.run(&mut func, &i));
    let retains = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    assert_eq!(retains, 0);
}

#[test]
fn last_use_move_at_forward_join() {
    // s = "x"; if c { } else { }; t = s; return t; — move at the join after a diamond.
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.string());
    let s = b.new_local(i.string(), Some("s".into()));
    let t = b.new_local(i.string(), Some("t".into()));
    let then_blk = b.new_block();
    let else_blk = b.new_block();
    let join = b.new_block();
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
    );
    b.terminate(Terminator::If {
        cond: Operand::Const(crate::Const::Bool(true)),
        then_blk,
        else_blk,
    });
    b.switch_to(then_blk);
    b.terminate(Terminator::Goto(join));
    b.switch_to(else_blk);
    b.terminate(Terminator::Goto(join));
    b.switch_to(join);
    b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &i));
    let nulls = func
        .blocks
        .iter()
        .flat_map(|bb| &bb.stmts)
        .filter(|s| {
            matches!(
                s,
                Statement::Assign(_, Rvalue::Use(Operand::Const(crate::Const::Null)))
            )
        })
        .count();
    assert_eq!(nulls, 1, "expected move of s into t at the join");
}

#[test]
fn inserts_retain_on_borrowed_copy() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.string());
    let s = b.new_param(i.string(), Some("s".into()));
    let t = b.new_local(i.string(), Some("t".into()));
    b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &i));
    let retains = func.blocks[0]
        .stmts
        .iter()
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    assert!(
        retains >= 1,
        "returning a copy of a borrowed param must retain"
    );
}

#[test]
fn inserts_retain_on_borrowed_js_copy() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.js());
    let s = b.new_param(i.js(), Some("s".into()));
    let t = b.new_local(i.js(), Some("t".into()));
    b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &i));
    let retains = func.blocks[0]
        .stmts
        .iter()
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    assert!(
        retains >= 1,
        "returning a copy of a borrowed js param must retain"
    );
}

#[test]
fn returned_owned_local_is_not_released() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.string());
    let s = b.new_local(i.string(), Some("s".into()));
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(s)))));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &i);
    let releases = func.blocks[0]
        .stmts
        .iter()
        .filter(|s| matches!(s, Statement::Release(_)))
        .count();
    assert_eq!(
        releases, 0,
        "empty dest is not released; returned local keeps the token"
    );
    assert!(matches!(
        func.blocks[0].terminator,
        Terminator::Return(Some(Operand::Copy(Place::Local(l)))) if l == s
    ));
}

#[test]
fn last_use_move_skips_retain_and_nulls_source() {
    // s = "x"; t = s; return t;  — after the copy, s is dead (t is returned), so move.
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.string());
    let s = b.new_local(i.string(), Some("s".into()));
    let t = b.new_local(i.string(), Some("t".into()));
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
    );
    b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(t)))));
    let mut func = b.finish();
    assert!(RcInsertion.run(&mut func, &i));
    // Expect: assign s="x"; retain s; assign t=s; assign s=null; (return t, no release t)
    // String literal binding retains; move of s→t skips retain on t and nulls s.
    let kinds: Vec<&str> = func.blocks[0]
        .stmts
        .iter()
        .map(|s| match s {
            Statement::Release(_) => "release",
            Statement::Assign(Place::Local(_), Rvalue::Use(Operand::Const(crate::Const::Null))) => {
                "null"
            }
            Statement::Assign(..) => "assign",
            Statement::Retain(_) => "retain",
            _ => "other",
        })
        .collect();
    assert!(
        kinds.contains(&"null"),
        "expected null of moved source, got {:?}",
        kinds
    );
    // No retain of t after the t=s assign (the move). There is still retain of s for the literal.
    let retain_count = kinds.iter().filter(|k| **k == "retain").count();
    assert_eq!(
        retain_count, 1,
        "only the string-literal retain should remain: {:?}",
        kinds
    );
}

#[test]
fn no_move_when_source_still_live() {
    // s = "x"; t = s; return s; — s is live after the copy (returned), so cannot move.
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.string());
    let s = b.new_local(i.string(), Some("s".into()));
    let t = b.new_local(i.string(), Some("t".into()));
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
    );
    b.assign(Place::Local(t), Rvalue::Use(Operand::Copy(Place::Local(s))));
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(s)))));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &i);
    let nulls = func.blocks[0]
        .stmts
        .iter()
        .filter(|s| {
            matches!(
                s,
                Statement::Assign(_, Rvalue::Use(Operand::Const(crate::Const::Null)))
            )
        })
        .count();
    assert_eq!(nulls, 0, "source still live — no move");
    // `t` is an unused forwarding alias of `s`; it is a cursor and must not retain.
    let retains = func.blocks[0]
        .stmts
        .iter()
        .filter(|s| matches!(s, Statement::Retain(_)))
        .count();
    assert_eq!(retains, 1, "only the string-literal retain: {:?}", retains);
}

#[test]
fn last_use_string_released_after_print() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let s = b.new_local(i.string(), Some("s".into()));
    let tmp = b.new_local(i.int(), Some("tmp".into()));
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
    );
    b.push(Statement::Print {
        arg: Operand::Copy(Place::Local(s)),
        ty: i.string(),
        newline: true,
    });
    b.push(Statement::SourceLine(2));
    b.assign(
        Place::Local(tmp),
        Rvalue::Binary(
            crate::BinOp::Add,
            Operand::Const(crate::Const::Int(1)),
            Operand::Const(crate::Const::Int(2)),
        ),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &i);
    let stmts = &func.blocks[0].stmts;
    let print_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Print { .. }))
        .unwrap();
    let add_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Assign(_, Rvalue::Binary(..))))
        .unwrap();
    let early_release = stmts.iter().enumerate().any(|(idx, st)| {
        idx > print_at
            && idx < add_at
            && matches!(st, Statement::Release(Operand::Copy(Place::Local(l))) if *l == s)
    });
    assert!(
        early_release,
        "last-use string after print must Release before later work: {:?}",
        stmts
    );
}

#[test]
fn early_release_after_last_js_use() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let h = b.new_local(i.js(), Some("h".into()));
    let tmp = b.new_local(i.int(), Some("tmp".into()));
    b.assign(
        Place::Local(h),
        Rvalue::Use(Operand::Const(crate::Const::Null)),
    );
    b.push(Statement::Print {
        arg: Operand::Copy(Place::Local(h)),
        ty: i.js(),
        newline: true,
    });
    b.push(Statement::SourceLine(2));
    b.assign(
        Place::Local(tmp),
        Rvalue::Binary(
            crate::BinOp::Add,
            Operand::Const(crate::Const::Int(1)),
            Operand::Const(crate::Const::Int(2)),
        ),
    );
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &i);
    let stmts = &func.blocks[0].stmts;
    let print_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Print { .. }))
        .unwrap();
    let add_at = stmts
        .iter()
        .position(|s| matches!(s, Statement::Assign(_, Rvalue::Binary(..))))
        .unwrap();
    let early = stmts.iter().enumerate().any(|(idx, st)| {
        idx > print_at
            && idx < add_at
            && matches!(st, Statement::Release(Operand::Copy(Place::Local(l))) if *l == h)
    });
    assert!(
        early,
        "expected Release of js handle between print and add, got {:?}",
        stmts
    );
}

#[test]
fn no_early_release_of_loop_carried_local() {
    let i = TypeInterner::new();
    let mut b = FunctionBuilder::new("f", i.void());
    let s = b.new_local(i.string(), Some("s".into()));
    let c = b.new_param(i.bool(), Some("c".into()));
    b.assign(
        Place::Local(s),
        Rvalue::Use(Operand::Const(crate::Const::Str("x".into()))),
    );
    let header = b.new_block();
    let body = b.new_block();
    let exit = b.new_block();
    b.terminate(Terminator::Goto(header));
    b.switch_to(header);
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(c)),
        then_blk: body,
        else_blk: exit,
    });
    b.switch_to(body);
    b.push(Statement::Print {
        arg: Operand::Copy(Place::Local(s)),
        ty: i.string(),
        newline: true,
    });
    b.terminate(Terminator::Goto(header));
    b.switch_to(exit);
    b.terminate(Terminator::Return(None));
    let mut func = b.finish();
    RcInsertion.run(&mut func, &i);
    let body_stmts = &func.blocks[body.0 as usize].stmts;
    let body_early = body_stmts
        .iter()
        .any(|st| matches!(st, Statement::Release(Operand::Copy(Place::Local(l))) if *l == s));
    assert!(
        !body_early,
        "loop-carried s must not be released in the body: {:?}",
        body_stmts
    );
}
