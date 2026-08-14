# 04 — CFG MIR (`src/mir/`)

MIR is where Dream becomes optimizable. It replaces structured control flow with an explicit **control-flow graph**. Heap lifetime is still implicit in HIR; after optimization, `InsertDrops` inserts `$dream_drop` for owning locals. Once a program is in MIR, ordinary dataflow analysis can reason about it.

## Mental model

```mermaid
flowchart TD
    subgraph "MirFunction"
      direction TB
      e[entry: BlockId]
      b0["bb0\nstmts...\nIf{cond, bb1, bb2}"]
      b1["bb1\nstmts...\nGoto bb3"]
      b2["bb2\nstmts...\nGoto bb3"]
      b3["bb3\nstmts...\nReturn"]
    end
    e --> b0
    b0 -->|then| b1
    b0 -->|else| b2
    b1 --> b3
    b2 --> b3
```

- A function is a list of `BasicBlock`s plus an `entry` block id.
- Each block is `stmts: Vec<Statement>` then exactly one `terminator: Terminator`. Control can branch *only* at the terminator.
- Values live in `Local`s. Every intermediate result is materialized into a local, so an `Operand` is always a local/global read or a constant — never a nested computation. This flattening is what makes passes simple.

## Core types (`src/mir/mod.rs`)

### Statements — straight-line, no control flow

```rust
pub enum Statement {
    Assign(Place, Rvalue),  // place = rvalue
    ForceFree(Operand),     // $dream_drop (nested deinit + $free)
    Call { callee, args },  // call for effect; return value discarded
    Nop,                    // tombstone left by passes that delete without renumbering
}
```

### Terminators — exactly one per block

```rust
pub enum Terminator {
    Goto(BlockId),
    If { cond: Operand, then_blk, else_blk },
    Switch { value: Operand, targets: Vec<(i64, BlockId)>, default: BlockId },  // → br_table
    Return(Option<Operand>),
    Unreachable,   // #[default]
}
```

`Terminator::successors()` is the one place CFG edges are defined — every traversal (passes, DCE, relooper) goes through it, so adding a terminator variant means updating exactly one function.

### Places, operands, constants

- `Place` (assignable): `Local`, `Global`, `Field { base, field }`, `Index { base, index: Box<Operand> }`. *(The `Box` breaks the `Place`→`Operand`→`Place` type cycle.)*
- `Operand` (readable): `Copy(Place)` or `Const(Const)`.
- `Const`: `Int`, `Float`, `Bool`, `Char`, `Str(String)` (interned later), `Null` (pointer-sized zero — an absent heap reference / cleared slot in MIR, **not** a source-level `null` literal; the language uses `Option<T>` / `None` for absence).

### Rvalues — all real computation

```rust
pub enum Rvalue {
    Use(Operand),
    Binary(BinOp, Operand, Operand),
    Unary(UnOp, Operand),
    Call { callee, args },
    IndirectCall { target, args },
    New { def, args },                  // allocate + construct a struct
    UnionNew { def, variant, args },
    ArrayLit { elem_ty, elems },
    ArrayLen(Operand),
    Cast(Operand, TypeId),
}
```

`Callee { def, args, ret }` carries the resolved def, the concrete type args (for monomorphization), and the site return type. The emitted symbol name is derived from `(def, args)` at the backend.

## Lowering HIR → MIR (`src/mir/lower/`)

`lower_program(hir, interner)` lowers each `HFunction` via `lower_function`; the `Lowerer` holds the block list and a "current block" cursor and appends statements as it walks the structured HIR.

The essential trick is that **every structured construct becomes blocks + terminators**:

```mermaid
flowchart TD
    subgraph "if cond { T } else { E }; after"
      H[cur block: eval cond] --> T1[then block: lower T]
      H --> E1[else block: lower E]
      T1 --> J[join block: continue]
      E1 --> J
    end
```

| HIR | MIR shape |
|-----|-----------|
| `If` | cur → `If{cond, then, else}`; both arms `Goto` a fresh join block |
| `While` | header block tests cond → body / exit; body `Goto`s header (back-edge) |
| `For` | init in cur; then a `While`-shaped header with the step appended to the body |
| `Foreach` | desugars to an index local + bounds check + `Index` read into the elem local |
| `Switch` | `Switch` terminator with `(value, block)` targets + default |
| `&&` / `\|\|` | short-circuit: a branch that skips the rhs block |
| `??` (`Coalesce`) | null-test branch choosing lhs or rhs |
| `Ternary` | same as `if` but both arms assign one result local |

Expression lowering (`lower_expr`) returns an `Operand`: literals become `Const`; everything composite is assigned into a fresh temporary local and the temp is returned. `break`/`continue` consult a stack of `(break_target, continue_target)` block ids maintained around loops.

`is_reference(ty)` delegates to `interner.is_reference` — the same single source of truth used everywhere else.

## Why drops are explicit in MIR

`InsertDrops` runs **after** the per-function opt pipeline (`PassManager::run_module`), using whole-module escape info so callers do not drop values that a callee stored or returned as an alias. Unique array locals are also dropped on overwrite.

```mermaid
flowchart LR
    A["buf = Buffer.alloc\n... use buf ...\nbuf = Buffer.alloc"] -->|InsertDrops| B["force_free buf\nbuf = Buffer.alloc"]
```

See [05-writing-passes.md](./05-writing-passes.md) and [12-allocators.md](./12-allocators.md).

## Building MIR by hand — `src/mir/build.rs`

`FunctionBuilder` is the ergonomic constructor used by tests and anything that synthesizes MIR directly (e.g. compiler-generated trampolines). It hands out fresh `Local`s and `BlockId`s, lets you push statements into the current block, and finalizes a `MirFunction`. Use it instead of building the structs by hand — it keeps the locals/blocks vectors consistent.

## Pretty-printing — `src/mir/print.rs`

MIR has a textual dump for debugging and snapshot tests. When a pass misbehaves, print the function before and after; the CFG dump is far easier to read than the WAT.

## Invariants MIR guarantees to the backend

1. Every block ends in exactly one terminator; `entry` is a valid block id.
2. Operands are atomic (local/global/const) — no nested computation hides in an operand.
3. Every `Local` has a `LocalDecl` with a valid `TypeId`.
4. The CFG is **reducible** (Dream cannot express `goto` spaghetti), so the relooper always succeeds.
5. After `InsertDrops`, owning heap locals that do not escape are dropped on every exit path (and unique arrays on overwrite).
