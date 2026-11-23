use super::Case;

pub(super) const CASES: &[Case] = &[
    Case::error(
        "attribute_errors",
        "duplicate '@inline' attribute\n'@json' cannot be applied to a top-level function\nan extern function cannot combine `@c`, `@js`, `@runtime`, `@intrinsic`, or `@llvm`\nunknown attribute '@bogus'\n'@js' expects 2 argument(s), got 1\n'@route' argument 1 must be a string literal\n'@route' cannot be applied to a class\nunknown attribute '@page'\n'@route' expects 1 argument(s), got 2\n'@marshal' argument 1 must be a member of 'StringEncoding'\n'@gpu_targets' argument 2 must be a member of 'GpuTarget', got 'GpuTarget.OpenGl'\n'@allow_cycle' does not take any arguments\n'@allow_cycle' cannot be applied to a struct\n'@generator' does not take any arguments",
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
        "cfg_errors",
        "unknown target 'Target.Freebsd'; expected one of Target.Debug, Target.Release, Target.Windows, Target.Macos, Target.Linux, Target.Android, Target.Ios, Target.Native, Target.Wasm, Target.Browser, Target.Node, Target.Wasi\n'@cfg' conditions test 'Target.<name>', but found 'Os'\nexpected '&&', '||', or ')' in a '@cfg' condition but found identifier\nan item can have only one '@cfg'; combine the conditions with '&&' or '||'\n'@cfg' must be the first attribute of a declaration, member, or statement\n'@cfg' must be followed by the declaration, member, or statement it controls",
        true,
    ),
    Case::error(
        "cfg_target_outside",
        "'Target' is only valid inside '@cfg(...)'; to compile code for one target, put '@cfg(Target.<name>)' on the declaration, member, or statement",
        true,
    ),
    Case::error(
        "escape_errors",
        "unknown escape '\\q'; write '\\\\' for a literal backslash\nescape_errors.dream:5:20\na '\\u' escape is written '\\u{XXXX}' with 1 to 6 hex digits\n'\\u{D800}' is not a Unicode scalar value\nunknown escape '\\x'\n'\\u{110000}' is not a Unicode scalar value\nescape_errors.dream:9:13",
        true,
    ),
    Case::error(
        "field_default_errors",
        "constructor of 'Partial' must assign field 'who' on every path: 'Speaker' has no default value\n'NoCtor' needs a constructor that assigns field 'at': 'Point' has no default value",
        true,
    ),
    Case::error(
        "raw_string_errors",
        "a multi-line raw string must start on the line after its opening quotes\nthe closing quotes of a multi-line raw string must be on their own line\na raw string line must start with the same whitespace as its closing quotes\nraw string literal closes with more quotes than it opens with; open it with \"\"\"\"",
        true,
    ),
    Case::error(
        "call_errors",
        "No overload of 'Point.constructor' matches argument types (Point, string)\nAmbiguous call to 'g'\nAmbiguous named-argument call to 'g'\n'add' is overloaded; give it a fun(...) type to select an overload\nNo overload of 'add' takes parameters (bool, bool)\nNo overload of 'h'\nno parameter named 'nam'\nvariadic parameter 'nums' cannot be passed by name\nfunction 'takes_ref' expects parameter 1 to be passed with 'ref', but it was passed by value\nfunction 'takes_value' expects parameter 1 to be passed by value, but 'ref' was given\n'clear' is an instance method of 'Counter'; call it on a 'Counter' value, not on the type name\nfunction 'take' expects parameter 1 to be List<List<int>>, got List<int>",
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
        "reference cycle: Node<int>.next -> Node<int>\nmake 'Node.next' weak or unowned\nreference cycle: Owner.handler -> Handler -> HandlerImpl.owner -> Owner\nreference cycle: Data.r -> (Data, int) -> Data\nreference cycle: Back.back -> C.h -> Holder.d -> Back\nreference cycle: Button.on_click -> closure fun(): void -> Button\nreference cycle: Peer.friend -> Peer\ncycle_errors.dream:112\nreference cycle: Node<string>.next -> Node<string>",
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
        "'List<Widget>.sort' requires 'Widget' to satisfy 'Comparable<Widget>'\nfunction Map<int, int>.get expects parameter 1 to be int, got ReadOnlySpan<char>",
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
        "capturing lambdas cannot be passed to JS APIs; use a captureless function",
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
        "type 'string' does not satisfy the 'unmanaged' constraint on generic parameter 'T'\nunknown type 'MissingType'\nalignof: unknown type 'MissingType'\nsizeof: unknown type 'Banana'",
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
        "empty character literal\ncharacter literal may only contain one character\nunknown escape '\\q'\nExpected expression but found ColonToken\nExpected ';' but found ','\n'static' cannot modify a top-level variable; it declares class members. Top-level variables are file-private by default — use 'internal' or 'public' to widen\n'ref' may only precede 'struct', not 'class'\n'ref' parameter 'x' cannot have a default value\nparameter 'xs' cannot be both 'ref' and variadic\n'string' is a reserved word and cannot be used as a variable name\nvariadic parameter 'n' must have an array type, e.g. '...n: T[]'\nparameter 'extra' cannot follow the variadic parameter; the variadic parameter must be last",
        true,
    ),
    Case::error(
        "private_member_errors",
        "'_secret' is private to 'Box'\n'reveal' is private to 'Box'\nconstructor of 'Hidden' is not accessible here",
        true,
    ),
    Case::error_targets(
        "cfg_host_availability",
        Some("Function does not exist (browser_only)"),
        Some("Function does not exist (native_only)"),
    ),
    Case::error("sizeof_case_duplicates", "duplicate case label '8'", true),
    Case::error(
        "span_escape_errors",
        "async function 'sum' cannot take 'ref struct' parameter 'p' of type 'Pair'\ncannot capture 'p' of type 'Pair' in a lambda expression: 'Pair' is a 'ref struct'\nfield 'pair' of 'Holder' cannot have type 'Pair': a 'ref struct' cannot be stored as a field\n'Pair' is a 'ref struct' and cannot be used as a generic type argument\n'ReadOnlySpan<char>' is a 'ref struct' and cannot be an array element type\n'ReadOnlySpan<char>' is a 'ref struct' and cannot be used as a generic type argument\na hidden loop variable of 'ref struct' type 'Span<int>' is still in scope at this 'await'\n'v' of 'ref struct' type 'ReadOnlySpan<char>' is still in scope at this 'await'\ncannot capture 'ref' parameter 'x' in a lambda expression (its storage is only valid for the call's duration)\ntype 'ReadOnlySpan<char>' is not index-assignable",
        true,
    ),
    Case::error(
        "static_field_errors",
        "a field cannot be 'static'; declare shared state as a module-level `let`, or expose it through a static property\nstatic_field_errors.dream:5:12",
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
        "cannot capture 'wrap' of type 'Wrap' in a `Task.spawn` body: 'Wrap' is not shared — mark the class 'shared', capture a blittable value, string, or a struct of those, or pass a heap object by move (its binding cannot be used afterwards)\ncannot capture 'wrap' of type 'Wrap' in a `TaskScope<string>.spawn_parallel` body: 'Wrap' is not shared",
        true,
    ),
    Case::error(
        "unsafe_call_errors",
        "call to '@unsafe' function 'Arena.constructor' is only allowed from another '@unsafe' function or method\ncall to '@unsafe' function 'Arena.alloc'\ncall to '@unsafe' function 'RawPtr<int>.indexer'\ncall to '@unsafe' function 'RawPtr<int>.cast'\ncall to '@unsafe' function 'Buffer.realloc' is only allowed from another '@unsafe' function or method\ncall to '@unsafe' function 'RawPtr<int>.alloc' is only allowed from another '@unsafe' function or method",
        true,
    ),
    Case::error(
        "diagnostic_hints_errors",
        "function 'greet' expects parameter 2 to be int, got string\ndiagnostic_hints_errors.dream:17:16\nType 'string' has no member 'lenght'\nsimilar member exists: 'string.length'\na variable with a similar name exists: 'value'\na function with a similar name exists: 'greet'\nadd 'import system;'\n'List<Key>.sort' requires 'Key' to satisfy 'Comparable<Key>'\nlambda parameter type mismatch: expected Key, got string\ndoes not satisfy the constraint 'Comparable<Key>'",
        true,
    ),
    Case::error(
        "float_literal_range_errors",
        "literal 1e400 does not fit in double\nfloat_literal_range_errors.dream:3:13\nliteral 3.4e39 does not fit in float\nliteral 1e-400 is too small for double and would round to 0\nliteral -1e309 does not fit in double",
        true,
    ),
    Case::error(
        "generic_call_reading_errors",
        "'a' is a variable, but 'a<…>(…)' is read as a generic call\ngeneric_call_reading_errors.dream:14:7",
        true,
    ),
    Case::error(
        "labeled_break_return_errors",
        "function 'f': not all code paths return a value\nlabeled_break_return_errors.dream:4:5",
        true,
    ),
    Case::error(
        "value_cycle_generic_errors",
        "value struct 'Wrap<Shape>' cannot contain itself by value\nvalue_cycle_generic_errors.dream:2:8",
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
        // The reported edge is the one `Map` visits second. Hash mixing puts `b` ahead of `a`.
        "dependency override must return the original provider's value type\n@dep(cyclic_provider) forms a cycle\n@dep(b) forms a cycle\nduplicate GET route '/same'\nHTTP route methods on a class must be static",
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
    Case::error(
        "enum_value_errors",
        "enum value '1.5' must be an integer in the int range\nenum value of 'B' overflows int; give it an explicit value\nenum variant 'B' has the same value 1 as 'A'\nenum variant 'C' has the same value 1 as 'A'\nvariant 'A' carries a payload and cannot have an explicit value",
        true,
    ),
    Case::error(
        "silent_type_errors",
        "class 'HiddenRunner' method 'run' implements interface 'Runner' and cannot be private\nthe get and set indexers of 'Lookup' disagree: get is [int]: string, set is [string] = int\ndefault value of parameter 'flag' has type 'int', but the parameter is 'bool'\ndefault value of parameter 'amount' has type 'string', but the parameter is 'int'\ncannot pass const 'fixed' by 'ref'\nCannot assign to 'fixed' because it is a const binding\n'count' is private to 'Counter'\n'ref' needs a variable, field or element, but 'total' is a temporary value\n'is string' can never be true for a value of type int\n'is Point' can never be true for a value of type Counter\nan integer range cannot match a 'float' subject; write the bounds as 'float' literals\n'this' cannot be assigned to\n'ref' arguments 1 and 2 may name the same storage; a field or element passed by 'ref' is copied back after the call, so one of the writes would be lost\nsilent_type_errors.dream:70:41\nsilent_type_errors.dream:71:29",
        true,
    ),
    Case::error(
        "type_context_errors",
        "cannot convert from string to int\ntype_context_errors.dream:16:12\noperator '+' cannot combine bool and int\noperator '*' cannot combine string and int\noperator '<' cannot combine string and int\noperator '&&' cannot combine int and bool\n! operator requires bool, got int\nwhile condition must be bool, got int\nTernary condition must be of type bool, got int\nfor condition must be bool, got int\ntype_context_errors.dream:38:5\ncannot convert from float to int\ntype_context_errors.dream:42:16\ntype_context_errors.dream:43:21\nfunction 'takes_int' expects parameter 1 to be int, got string\nstrings are immutable; build a new string with replace, substring, or StringBuilder\ntype_context_errors.dream:46:5",
        true,
    ),
    Case::error(
        "type_safety_errors",
        "Array index must be of type int, got string\nArray index must be of type int, got float\nFunction does not exist (missing_value)\na generic lambda that captures variables needs a known function type here\ncannot convert from Level to int\ncannot convert from int to Level\nfunction 'takes_level' expects parameter 1 to be Level, got int\nCannot cast from int to Holder\nCannot cast from int to int[]\noperator '+' cannot be applied to Level\nclass 'PlainAdjuster' method 'adjust' does not match the signature required by interface 'Adjuster'\nclass 'PlainAdjuster' method 'total' does not match the signature required by interface 'Adjuster'\nTop-level variable 'NARROWED' is declared 'int' but initialized with 'float'\nliteral -1 does not fit in byte\nunary '-' cannot be applied to unsigned type uint\ncannot convert from int to NumericScore\nCannot cast from int to interface NumericScore\nunknown type 'MissingTestType'",
        true,
    ),
    Case::error(
        "value_diagnostic_errors",
        "value_diagnostic_errors.dream:24:24\nvalue_diagnostic_errors.dream:25:25\nvalue_diagnostic_errors.dream:26:18\nvalue_diagnostic_errors.dream:27:20\nvalue_diagnostic_errors.dream:28:17\nvalue_diagnostic_errors.dream:29:16\nvalue_diagnostic_errors.dream:30:20\nthis expression returns 'void' and has no value to use\nif condition must be bool, got void\nvalue_diagnostic_errors.dream:36:12\ncannot convert from void to int\ncannot destructure non-tuple type int\nvariable undefined_index does not exist\ntype 'Plain' does not satisfy the constraint 'Comparable<Plain>' on generic parameter 'T'\npattern 'Other.Nothing' is a variant of 'Other', not of the matched enum 'Shape'\npattern 'Nothing' is a variant of 'Other', not of the matched enum 'Shape'\nVariant 'Shape.Circle' has 1 field(s), but the pattern binds 0",
        true,
    ),
];
