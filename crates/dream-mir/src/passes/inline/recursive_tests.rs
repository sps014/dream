use super::recursive;
use crate::build::FunctionBuilder;
use crate::{BinOp, Callee, Const, Mir, Operand, Place, Rvalue, Statement, Terminator};
use dream_abi::profile::CompileProfile;
use dream_types::{DefId, TypeInterner};

fn function(i: &TypeInterner) -> crate::MirFunction {
    let mut b = FunctionBuilder::new("recursive", i.int());
    b.set_def(DefId::root(1), vec![]);
    let n = b.new_param(i.int(), None);
    let cond = b.new_temp(i.bool());
    b.assign(
        Place::Local(cond),
        Rvalue::Binary(
            BinOp::Lt,
            Operand::Copy(Place::Local(n)),
            Operand::Const(Const::Int(2)),
        ),
    );
    let base = b.new_block();
    let recurse = b.new_block();
    b.terminate(Terminator::If {
        cond: Operand::Copy(Place::Local(cond)),
        then_blk: base,
        else_blk: recurse,
    });
    b.switch_to(base);
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(n)))));
    b.switch_to(recurse);
    let mut results = Vec::new();
    for delta in [1, 2] {
        let arg = b.new_temp(i.int());
        let result = b.new_temp(i.int());
        b.assign(
            Place::Local(arg),
            Rvalue::Binary(
                BinOp::Sub,
                Operand::Copy(Place::Local(n)),
                Operand::Const(Const::Int(delta)),
            ),
        );
        b.assign(
            Place::Local(result),
            Rvalue::Call {
                callee: Callee {
                    def: DefId::root(1),
                    args: vec![],
                    ret: i.int(),
                    take_params: vec![false],
                },
                args: vec![Operand::Copy(Place::Local(arg))],
            },
        );
        results.push(result);
    }
    let result = b.new_temp(i.int());
    b.assign(
        Place::Local(result),
        Rvalue::Binary(
            BinOp::Add,
            Operand::Copy(Place::Local(results[0])),
            Operand::Copy(Place::Local(results[1])),
        ),
    );
    b.terminate(Terminator::Return(Some(Operand::Copy(Place::Local(
        result,
    )))));
    b.finish()
}

#[test]
fn scalar_recursion_expands_only_original_edges() {
    let i = TypeInterner::new();
    let f = function(&i);
    let mut mir = Mir {
        profile: CompileProfile::Release,
        functions: vec![f],
        ..Mir::default()
    };
    assert!(recursive::run(&mut mir, &i));
    assert_eq!(mir.functions.len(), 1);
    let calls = mir.functions[0]
        .blocks
        .iter()
        .flat_map(|b| &b.stmts)
        .filter(|s| matches!(s, Statement::Assign(_, Rvalue::Call { .. })))
        .count();
    assert_eq!(calls, 4);
    crate::verify::assert_module(&mir, &i);
}

#[test]
fn recursion_respects_noinline_debug_and_opaque_effects() {
    let i = TypeInterner::new();
    for exclusion in 0..4 {
        let mut f = function(&i);
        if exclusion == 0 {
            f.inline = dream_hir::InlineHint::Never;
        }
        if exclusion == 2 {
            f.blocks[0]
                .stmts
                .push(Statement::Panic(Operand::Const(Const::Str("no".into()))));
        }
        let mut mir = Mir {
            profile: CompileProfile::Release,
            functions: vec![f],
            ..Mir::default()
        };
        if exclusion == 1 {
            mir.profile = CompileProfile::Debug;
        }
        if exclusion == 3 {
            mir.exports.push((DefId::root(1), "recursive".into()));
        }
        assert!(!recursive::run(&mut mir, &i));
        assert_eq!(mir.functions[0].blocks.len(), 3);
    }
}
