use super::Case;

pub(super) const CASES: &[Case] = &[
    Case::error(
        "attribute_errors",
        "duplicate '@inline' attribute\n'@json' cannot be applied to a top-level function\nan extern function cannot combine `@c`, `@js`, `@runtime`, `@intrinsic`, or `@llvm`\nunknown attribute '@bogus'\n'@js' expects 2 argument(s), got 1\n'@route' argument 1 must be a string literal\n'@route' cannot be applied to a class\nunknown attribute '@page'\n'@route' expects 1 argument(s), got 2\n'@c_call' argument 1 must be a member of 'CCallConv'\n'@gpu_targets' argument 2 must be a member of 'GpuTarget', got 'GpuTarget.OpenGl'\n'@allow_cycle' does not take any arguments\n'@allow_cycle' cannot be applied to a struct\n'@generator' does not take any arguments",
        true,
    ),
    Case::error(
        "await_defer_errors",
        "'await' can only be used inside an 'async' function\n'await' cannot be used inside 'defer'\n'break' cannot be used inside 'defer'\n'continue' cannot be used inside 'defer'\n'return' cannot be used inside 'defer'\ncannot convert from int to uint\n'await' cannot be used inside a 'lock' block",
        true,
    ),
    Case::error(
        "binding_errors",
        "Cannot assign to 'x' because it is a const binding\nCannot assign to 'LIMIT' because it is a const binding\n'_' is a discard and cannot be used as a value\ncannot bind 'r' to a void value\n'weak' local 'handle' must target a class",
        true,
    ),
    Case::error(
        "borrow_errors",
        "method 'Ledger.audit' is declared 'borrow' but its body mutates the instance\n'MemoryRepo.save' is inferred borrow but interface 'Repository' declares it 'unique'\ncannot call 'push' here",
        true,
    ),
    Case::error(
        "c_boundary_errors",
        "'@c' extern 'leaky' result: returning `OwnedCPtr` needs `@owned(\"free_fn\")` naming the C function that frees the pointer\n'@c' extern 'counted' result: `@owned(\"free_fn\")` applies only to a `@c` extern returning `OwnedCPtr`\n'@c' extern 'spaced' result: `@owned` names 'free me', which is not a C identifier\n'@c' extern 'reserved' binds 'dream_cs_x', which is not a C identifier\nargument 4 of '@c' extern 'qsort' must be a named function or a lambda literal\n'@c' extern 'puts' parameter 'lines': type 'List<string>' has no C representation here",
        true,
    ),
    Case::error(
        "call_errors",
        "No overload of 'Point.constructor' matches argument types (Point, string)\nAmbiguous call to 'g'\n'add' is overloaded; give it a fun(...) type to select an overload\nNo overload of 'add' takes parameters (bool, bool)\nNo overload of 'h'\nno parameter named 'nam'\nvariadic parameter 'nums' cannot be passed by name\nfunction 'takes_ref' expects parameter 1 to be passed with 'ref', but it was passed by value\nfunction 'takes_value' expects parameter 1 to be passed by value, but 'ref' was given\n'clear' is an instance method of 'Counter'; call it on a 'Counter' value, not on the type name\nfunction 'take' expects parameter 1 to be List<List<int>>, got List<int>",
        true,
    ),
    Case::error(
        "capability_errors",
        "validation requires an @json model\n'Map<string, object>' cannot be serialized to JSON: it has no compile-time JSON encoding. Use 'JsonValue' for mixed or unknown data, or mark a named type '@json'\ndeclof: 'Json.serialise' does not name a type, function, field, method or variant",
        true,
    ),
    Case::error(
        "class_decl_errors",
        "destructor 'del' must not declare parameters\nmethod 'to_string' overrides an object-protocol method; mark it with 'override'\ninterface inheritance cycle",
        true,
    ),
    Case::error(
        "class_export",
        "Public function 'get_internal' exposes private class 'InternalData'",
        true,
    ),
    Case::error(
        "collection_literal_errors",
        "a Map literal requires a target type, e.g. `let m: Map<string, int> = {\"a\": 1};`\na Set literal requires a target type, e.g. `let s: Set<int> = {1, 2};`\ncannot use a Set literal where a 'List<int>' is expected",
        true,
    ),
    Case::error(
        "cycle_errors",
        "class 'Node' can form a strong ownership cycle\nclass 'Owner' can form a strong ownership cycle\nclass 'HandlerImpl' can form a strong ownership cycle\nclass 'Data' can form a strong ownership cycle\nclass 'C' can form a strong ownership cycle",
        true,
    ),
    Case::error(
        "export_errors",
        "@export functions must be synchronous, non-generic and non-variadic\n@export name 'dream_retain' is reserved or is not a C identifier\nduplicate @export symbol 'duplicated'\nextern and @export functions cannot take or return 'float16'",
        true,
    ),
    Case::error(
        "gen_bad_parse",
        "Expected expression but found CloseParenthesisToken",
        true,
    ),
    Case::error(
        "gen_bad_replace",
        "Type 'int' has no method 'missing'",
        true,
    ),
    Case::error(
        "generator_attribute_not_attribute",
        "'NotAnAttribute' is not an @attribute type",
        true,
    ),
    Case::error(
        "generator_unknown_trigger",
        "'jsn' is not a declared @attribute type",
        true,
    ),
    Case::error(
        "generic_constraint_errors",
        "Type 'List<Widget>' has no method 'sort'\nfunction Map<int, int>.get expects parameter 1 to be int, got ReadOnlySpan<char>",
        true,
    ),
    Case::error(
        "gpu_scope_errors",
        "'global_id' is only available inside functions marked @compute\n'GpuBuffer<float>[]' is only available inside functions marked @compute",
        true,
    ),
    Case::error("import_error", "cannot convert from string to int", true),
    Case::error(
        "internal_item_errors",
        "Class 'ProcessWireReader' is not 'public'\nClass 'MapSlot' is not 'public'\nconstructor of 'Stdout' is not accessible here\n'zero' is not accessible\n'exchange' is not accessible\nType 'Weak' is not 'public'",
        true,
    ),
    Case::error(
        "is_dead_branch_checked",
        "Function does not exist (undefined_in_dead_branch)\ncannot convert from string to int",
        true,
    ),
    Case::error(
        "js_lambda_errors",
        "capturing lambdas cannot be passed to JS APIs (the closure environment would be lost); pass a non-capturing top-level function, or wrap only a captureless `fun(...)` via `js.func` / `js.func0`",
        true,
    ),
    Case::error(
        "json_constructor_mismatch",
        "@json type 'Pair' needs a constructor taking every field in declaration order, or a zero-argument constructor",
        true,
    ),
    Case::error(
        "json_field_not_json",
        "@json class 'Wrapper' field 'inner' has unsupported type 'Plain'; 'Plain' must itself be marked @json (add the @json attribute to it)",
        true,
    ),
    Case::error(
        "json_ignore_no_default",
        "@json class 'Bad' field 'home' has @json_ignore but type 'Address' has no zero default (use `Option<Address>` or remove @json_ignore)",
        true,
    ),
    Case::error(
        "lambda_context_errors",
        "async lambda requires a `fun(...): Future<T>` context, but the expected type returns 'int'\nsync lambda cannot be used where `fun(...): Future<T>` is expected; write `async (params) => ...` for an async lambda\ncannot infer the type of this lambda without a `fun(...)`-typed context (e.g. `let f: fun(int): int = (x: int) => x * x;`)",
        true,
    ),
    Case::error(
        "layout_builtin_errors",
        "'raw memory access' requires an unmanaged (blittable) type, but 'string' is not\nraw memory access requires a known element type\nalignof: unknown type 'MissingType'\nsizeof: unknown type 'Banana'",
        true,
    ),
    Case::error(
        "module_errors",
        "'private_only' in module 'priv.mod' is private; only 'public'/'internal' items can be imported with 'as'\nFunction 'h' is not 'public'; it is private to its file\n'x' is private to 'Vector'\nDuplicate overload: 'add' with the same parameter types is already defined",
        true,
    ),
    Case::error(
        "numeric_errors",
        "cannot convert from float to int\ncannot convert from double to float\ncannot convert from long to int\ncannot convert from float to float16\ncannot convert from int to float16\nliteral 70000.0 does not fit in float16\ninteger literal '99999999999999999999' is out of range or malformed\nliteral 300 does not fit in byte\n~ operator requires an integer operand (int/long/uint/ulong/byte), got bool\n'..' requires int operands",
        true,
    ),
    Case::error(
        "operator_errors",
        "Ambiguous operator '+' on 'Ambiguous' with right operand 'int'\n'Vector2' already declares an operator overload for '+' on 'Vector2'\n'DuplicateReturn' already declares an operator overload for '+' on 'DuplicateReturn' with the same parameter types\n'operator ==' must return bool\nNo overload of operator '+' on 'Value' matches right operand 'bool'\nNo overload of operator '+' on 'Value' matches right operand 'Value'\n'operator &&' is not a recognized operator for a method with 1 parameter(s)\ncast method 'implicit' must declare a return type (the cast's target type)",
        true,
    ),
    Case::error(
        "parse_errors",
        "empty character literal\ncharacter literal may only contain one character\nunknown character escape '\\q'\nExpected expression but found ColonToken\nExpected ';' but found ','\n'static' cannot modify a top-level variable; it declares class members. Top-level variables are file-private by default — use 'internal' or 'public' to widen\n'ref' may only precede 'struct', not 'class'\n'ref' parameter 'x' cannot have a default value\nparameter 'xs' cannot be both 'ref' and variadic\n'string' is a reserved word and cannot be used as a variable name\nvariadic parameter 'n' must have an array type, e.g. '...n: T[]'\nparameter 'extra' cannot follow the variadic parameter; the variadic parameter must be last",
        true,
    ),
    Case::error(
        "private_member_errors",
        "'_secret' is private to 'Box'\n'reveal' is private to 'Box'\nconstructor of 'Hidden' is not accessible here",
        true,
    ),
    Case::error_targets(
        "runtime_attr_web_only",
        Some("'f' is not available on compile target 'native' (available on: web)"),
        Some("'f' is not available on compile target 'node' (available on: web)"),
    ),
    Case::error("sizeof_case_duplicates", "duplicate case label '8'", true),
    Case::error(
        "span_escape_errors",
        "async function 'sum' cannot take 'ref struct' parameter 'p' of type 'Pair'\ncannot capture 'p' of type 'Pair' in a lambda expression: 'Pair' is a 'ref struct'\nfield 'pair' of 'Holder' cannot have type 'Pair': a 'ref struct' cannot be stored as a field\n'Pair' is a 'ref struct' and cannot be used as a generic type argument\n'ReadOnlySpan<char>' is a 'ref struct' and cannot be an array element type\n'ReadOnlySpan<char>' is a 'ref struct' and cannot be used as a generic type argument\na hidden loop variable of 'ref struct' type 'Span<int>' is still in scope at this 'await'\n'v' of 'ref struct' type 'ReadOnlySpan<char>' is still in scope at this 'await'\ncannot capture 'ref' parameter 'x' in a lambda expression (its storage is only valid for the call's duration)\ntype 'ReadOnlySpan<char>' is not index-assignable",
        true,
    ),
    Case::error(
        "switch_arm_errors",
        "Unreachable switch arm: a previous arm already matches everything\nvariable undeclared does not exist\nfunction 'f': not all code paths return a value",
        true,
    ),
    Case::error(
        "syntax_block_unregistered",
        "unexpanded syntax block 'html'",
        true,
    ),
    Case::error(
        "task_capture_errors",
        "cannot capture 'wrap' of type 'Wrap' in a `Task.spawn` body: 'Wrap' is not shared — mark the class 'shared', capture a blittable value, string, or a struct of those, or pass a heap object by move (its binding cannot be used afterwards)\ncannot capture 'wrap' of type 'Wrap' in a `TaskScope_string.spawn_parallel` body: 'Wrap' is not shared",
        true,
    ),
    Case::error(
        "unsafe_call_errors",
        "call to '@unsafe' function 'Arena_constructor' is only allowed from another '@unsafe' function or method\ncall to '@unsafe' function 'Arena_alloc'\ncall to '@unsafe' function 'RawPtr_int___get_indexer'\ncall to '@unsafe' function 'RawPtr_int_cast'\ncall to '@unsafe' function 'Buffer.realloc' is only allowed from another '@unsafe' function or method\ncall to '@unsafe' function 'RawPtr_int_alloc' is only allowed from another '@unsafe' function or method",
        true,
    ),
    Case::error(
        "value_recursion_errors",
        "value struct 'Node' cannot contain itself by value\n'enum struct List' cannot be stored inline: field 'tail' has type 'List', which is a self-referential payload that would make this value type infinite-size",
        true,
    ),
    Case::error(
        "visibility_errors",
        "Type 'HiddenCache' is not 'public'; it is private to its file\n'locked' is private to 'OpenCache<int>'\nClass 'PrivateBox' is not 'public'; it is private to its file\nFunction 'private_secret' is not 'public'; it is private to its file\n'hidden' is private to 'Gadget'",
        true,
    ),
    Case::error(
        "webapi_collect_errors",
        "dependency override must return the original provider's value type\n@dep(cyclic_provider) forms a cycle\n@dep(a) forms a cycle\nduplicate GET route '/same'\nHTTP route methods on a class must be static",
        false,
    ),
    Case::error(
        "webapi_route_errors",
        "@body parameter 'value' must be @json, string, byte[], or JsonValue\nparameter 'value' is not a path segment of '/' and has no extractor\npath placeholder '{id}' has no matching parameter\n@use(missing) does not resolve to a function\n@use(wrong) must be\n@body cannot be combined with @form/@file\n@file parameter 'file' must be UploadedFile\n@websocket handler must take a ServerWebSocket parameter",
        false,
    ),
    Case::error(
        "yield_invalid",
        "'yield' requires an Iterable<T> or async AsyncIterable<T> function\nfunction YieldChannel<int>.emit expects parameter 1 to be int, got string\nuse 'yield break' to end an iterator\niterator parameters cannot be ref or borrow\n'await for' requires an async function\nan async iterator body must contain 'yield' or 'yield break'\n'yield' cannot suspend inside 'lock' or 'defer'\n'await for' requires AsyncIterable<T> or AsyncIterator<T>\niterator signal intrinsics are reserved for internal stdlib state",
        true,
    ),
];
