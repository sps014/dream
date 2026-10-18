use crate::format;

fn assert_format(input: &str, expected: &str) {
    let out = format(input);
    assert_eq!(out, expected, "input was:\n{input}");
}

/// format(format(x)) == format(x) for every fixture in this module.
fn assert_idempotent(input: &str) {
    let once = format(input);
    let twice = format(&once);
    assert_eq!(once, twice, "not idempotent for input:\n{input}");
}

#[test]
fn pretty_prints_minified_function() {
    assert_format(
        "fun main():void{let x:int=1;if(x>0){println(x);}}",
        "\
fun main(): void {
    let x: int = 1;
    if (x > 0) {
        println(x);
    }
}
",
    );
}

#[test]
fn else_joins_closing_brace() {
    let out = format("fun f():void{if(true){return;}else{return;}}");
    assert!(
        out.contains("} else {"),
        "expected closing brace + ` else {{`, got:\n{}",
        out
    );
}

#[test]
fn do_while_joins_closing_brace() {
    let out = format("fun f():void{do{x=x+1;}while(x<10);}");
    assert!(out.contains("} while (x < 10);"), "got:\n{}", out);
}

#[test]
fn preserves_line_comments() {
    let src = "// header\nfun main(): void { let x = 1; // trail\n}";
    let out = format(src);
    assert!(out.starts_with("// header\n"), "got:\n{}", out);
    assert!(out.contains("let x = 1; // trail"), "got:\n{}", out);
}

#[test]
fn preserves_block_comments_with_star_alignment() {
    let src = "/*\n * top\n * doc\n */\nfun main():void{}";
    let out = format(src);
    assert!(
        out.contains("/*\n * top\n * doc\n */"),
        "star alignment lost, got:\n{}",
        out
    );
}

#[test]
fn keeps_for_header_semicolons_inline() {
    let out = format("fun main():void{for(let i=0;i<10;i=i+1){println(i);}}");
    assert!(
        out.contains("for (let i = 0; i < 10; i = i + 1)"),
        "for header should stay one line, got:\n{}",
        out
    );
}

#[test]
fn blank_line_between_top_level_functions() {
    assert_format(
        "fun a():void{}\nfun b():void{}",
        "\
fun a(): void {}

fun b(): void {}
",
    );
}

#[test]
fn minified_decls_get_separated_by_one_blank_line() {
    let out = format("fun a():void{}fun b():void{}");
    assert!(out.contains("}\n\nfun b"), "got:\n{}", out);
}

#[test]
fn consecutive_imports_stay_packed() {
    assert_format(
        "import system;\nimport system.json;\nimport system.io;\nfun main(): void {}",
        "\
import system;
import system.json;
import system.io;

fun main(): void {}
",
    );
}

#[test]
fn generics_are_not_spaced_like_comparisons() {
    let out = format("let m:Map<string,int> = Map.new<string,int>();");
    assert!(
        out.contains("Map<string, int>") && !out.contains("Map < string"),
        "got:\n{}",
        out
    );
}

#[test]
fn comparisons_stay_spaced() {
    let out = format("let b:bool=a<c&&c>d;");
    assert!(out.contains("a < c && c > d"), "got:\n{}", out);
}

#[test]
fn nested_generic_call_arguments() {
    let out = format("process<Pair<Box<int>,int>,string>(pair);");
    assert!(
        out.contains("process<Pair<Box<int>, int>, string>(pair);"),
        "got:\n{}",
        out
    );
}

#[test]
fn signed_literals_bind_tight() {
    let out = format("let a:int=-1;let b:int=f(-2,+3);");
    assert!(out.contains("= -1;"), "got:\n{}", out);
    assert!(out.contains("f(-2, +3);"), "got:\n{}", out);
}

#[test]
fn switch_cases_are_indented() {
    assert_format(
        "fun pick(x:int):int{switch(x){case 1:return 10;case 2:{return 20;}default:return 0;}}",
        "\
fun pick(x: int): int {
    switch (x) {
        case 1:
            return 10;
        case 2: {
            return 20;
        }
        default:
            return 0;
    }
}
",
    );
}

#[test]
fn user_blank_lines_inside_blocks_collapse_to_one() {
    assert_format(
        "fun f():void{\n\n\n    let a = 1;\n\n\n    let b = 2;\n}",
        "\
fun f(): void {
    let a = 1;

    let b = 2;
}
",
    );
}

#[test]
fn empty_braces_stay_on_one_line() {
    assert_format(
        "class Empty{}",
        "\
class Empty {}
",
    );
}

#[test]
fn chained_calls_have_no_spaces_around_dots() {
    let out = format("let s:string=list.map(f).filter(g).join(\", \");");
    assert!(out.contains("list.map(f).filter(g).join"), "got:\n{}", out);
}

#[test]
fn short_postfix_await_chains_stay_compact() {
    let out =
        format("async fun f():void{let x=xyz().mzk().await.mka();let y=Time.sleep(1).await?;}");
    assert!(out.contains("xyz().mzk().await.mka()"), "got:\n{}", out);
    assert!(out.contains("Time.sleep(1).await?;"), "got:\n{}", out);
}

#[test]
fn trailing_comment_at_end_of_file_is_kept() {
    let out = format("fun main():void{}\n// done\n");
    assert!(out.ends_with("// done\n"), "got:\n{}", out);
}

#[test]
fn unlexable_input_is_returned_unchanged() {
    // `$` alone is not valid lexically anywhere outside an interpolated string prefix.
    let broken = "fun main():void{ let x = ` }";
    assert_eq!(format(broken), broken);
}

#[test]
fn empty_and_whitespace_only_input() {
    assert_eq!(format(""), "\n");
    assert_eq!(format("   \n\n  "), "\n");
}

#[test]
fn attributes_keep_decl_spacing() {
    let out = format("@json enum Shape{Circle,Square}enum Other{A}");
    assert!(out.contains("@json\nenum Shape {"), "got:\n{}", out);
    assert!(out.contains("}\n\nenum Other {"), "got:\n{}", out);
}

#[test]
fn parameter_attributes_with_arguments_stay_apart_from_the_parameter() {
    let out = format("fun f(@readonly @coop_shape(16, 32) a: int, @location(0) p: int, q: int): int { return (int)q; }");
    assert!(
        out.contains("@readonly @coop_shape(16, 32) a: int, @location(0) p: int"),
        "got:\n{}",
        out
    );
    assert!(out.contains("return (int)q;"), "got:\n{}", out);
}

#[test]
fn idempotency_over_all_fixtures() {
    let fixtures = [
        "fun main():void{let x:int=1;if(x>0){println(x);}}",
        "fun f():void{do{x=x+1;}while(x<10);}",
        "@json enum Shape{Circle,Square}enum Other{A}",
        "fun pick(x:int):int{switch(x){case 1:return 10;default:return 0;}}",
        "let m:Map<string,int>=Map.new<string,int>();",
        "// header\nfun main(): void { let x = 1; /* mid */ let y = 2; }\n// tail\n",
        "class A{fun m():int[]{return [];}}\nstruct P<T>{x:T}",
        "async fun f():void{let c=g().await?;let d=g().await[0];g().await;}",
        "fun f():void{let a=g()?;let t=c?x:y;}",
    ];
    for input in fixtures {
        assert_idempotent(input);
    }
}

#[test]
fn postfix_await_binds_tight() {
    let out = format(
        "async fun f():void{let b=g().await;let c=g().await?;let d=g().await[0];let e=g().await+1;}",
    );
    assert!(out.contains("let b = g().await;"), "got:\n{}", out);
    assert!(out.contains("let c = g().await?;"), "got:\n{}", out);
    assert!(out.contains("let d = g().await[0];"), "got:\n{}", out);
    assert!(out.contains("let e = g().await + 1;"), "got:\n{}", out);
}

#[test]
fn try_question_mark_hugs_its_operand_but_ternary_does_not() {
    let out = format("fun f():void{let a=g()?;let t=c?x:y;}");
    assert!(out.contains("let a = g()?;"), "got:\n{}", out);
    assert!(out.contains("let t = c ?"), "got:\n{}", out);
}

#[test]
fn formats_yield_and_async_iteration_without_changing_tokens() {
    let source = "async fun values():AsyncIterable<int>{yield -1;yield break;} async fun main(){await for(let x in values()){use(x);}}";
    let formatted = format(source);
    assert!(formatted.contains("yield -1;"));
    assert!(formatted.contains("yield break;"));
    assert!(formatted.contains("await for ("));
    assert_idempotent(source);
    let tokens = |text: &str| {
        let mut diagnostics = dream_diagnostics::DiagnosticBag::new(None);
        let result = dream_syntax::lexer::Lexer::new(text.to_string()).lex_all(&mut diagnostics);
        assert!(!diagnostics.has_errors());
        result
            .into_iter()
            .map(|token| (token.kind, token.text))
            .collect::<Vec<_>>()
    };
    assert_eq!(tokens(source), tokens(&formatted));
}

#[test]
fn wraps_long_parameter_and_argument_lists() {
    let source = "public fun build(first_parameter: string, second_parameter: string, third_parameter: string, fourth_parameter: string): string { return combine(first_parameter, second_parameter, third_parameter, fourth_parameter); }";
    let formatted = format(source);
    assert!(
        formatted.contains("build(\n    first_parameter: string,\n"),
        "{formatted}"
    );
    assert!(
        formatted.lines().all(|line| line.chars().count() <= 100),
        "{formatted}"
    );
    assert_idempotent(source);
}

#[test]
fn closure_closers_keep_call_punctuation_attached() {
    assert_format(
        "fun main(){run((x)=>{return x;}, [1,2]);}",
        "fun main() {\n    run(\n        (x) => {\n            return x;\n        },\n        [1, 2]\n    );\n}\n",
    );
}

#[test]
fn empty_block_with_trailing_comment_is_not_collapsed() {
    let source = "fun main(){if true { // intentional\n}}";
    assert!(format(source).contains("{ // intentional\n"));
    assert_idempotent(source);
}

#[test]
fn unary_signs_do_not_become_increment_or_decrement() {
    assert_format(
        "let x = - -1; let y = + +2;",
        "let x = - -1;\nlet y = + +2;\n",
    );
}

#[test]
fn shared_override_and_operator_keywords_keep_word_boundaries() {
    let source = "shared public class Value { public override fun to_string(): string { return \"v\"; } public fun operator ==(other: Value): bool { return true; } }";
    let result = format(source);
    assert!(result.contains("shared public class"));
    assert!(result.contains("public override fun"));
    assert!(result.contains("operator ==("));
    assert_idempotent(source);
}

#[test]
fn nested_switch_does_not_clear_the_outer_case_indent() {
    let result =
        format("fun f(){switch(x){case 1: switch(y){case 2: run();} next(); default: done();}}");
    assert!(result.contains("\n            next();"), "{result}");
    assert_idempotent(&result);
}

#[test]
fn documentation_stays_above_members_instead_of_trailing_previous_tokens() {
    assert_format(
        "class C {\n    /// First member.\n    public x: int;\n    /// Second member.\n    public fun f() {}\n\n    /// Third member.\n    public fun g() {}\n}",
        "class C {\n    /// First member.\n    public x: int;\n\n    /// Second member.\n    public fun f() {}\n\n    /// Third member.\n    public fun g() {}\n}\n",
    );
}

#[test]
fn generic_signature_fragments_keep_type_spacing() {
    assert_format(
        "public value: Option<string>",
        "public value: Option<string>\n",
    );
    assert_format("public class List<T>", "public class List<T>\n");
    assert_format(
        "fun f(token: Option<CancellationToken> = Option.None) {}",
        "fun f(token: Option<CancellationToken> = Option.None) {}\n",
    );
    assert_format(
        "let callback: NativeCallback<fun(int): int>;",
        "let callback: NativeCallback<fun(int): int>;\n",
    );
}

#[test]
fn ternary_branches_have_spaces_and_try_propagation_stays_tight() {
    assert_format(
        "let value = ready?yes:no;",
        "let value = ready ? yes : no;\n",
    );
    assert_format("let value = run()?;", "let value = run()?;\n");
}

#[test]
fn unicode_comments_do_not_move_documentation_onto_previous_statements() {
    let source = format!(
        "// {}\nclass C {{\n    /// First field.\n    public x: int;\n    /// Second field.\n    public y: int;\n    fun f() {{ use(); }}\n\n    /// Next method.\n    public fun g() {{ use(); }}\n}}\n",
        "界🦀—".repeat(80)
    );
    let result = format(&source);
    assert!(
        result.contains("{\n    /// First field.\n    public x: int;\n\n    /// Second field."),
        "{result}"
    );
    assert!(
        result.contains("}\n\n    /// Next method.\n    public fun g()"),
        "{result}"
    );
    assert_idempotent(&source);
}

#[test]
fn return_types_contribute_to_signature_width() {
    let source = "public async fun send(request: HttpRequest, token: Option<CancellationToken> = Option.None): Result<HttpResponse, HttpError> {}";
    let result = format(source);
    assert!(result.contains("send(\n"), "{result}");
    assert!(
        result.lines().all(|line| line.chars().count() <= 100),
        "{result}"
    );
    assert_idempotent(source);
}

#[test]
fn wrapped_chains_stay_vertical_after_multi_argument_calls() {
    let source = "let command = Command(\"download\").version(\"1.0.0\").description(\"Download a file\").positional(\"url\", required: true).string_option(\"output\", short: \"o\", default_value: \"download.bin\").int_option(\"timeout\", default_value: 10000).flag(\"verbose\", short: \"v\");";
    let result = format(source);
    assert!(
        result.contains("\n    .int_option(\"timeout\", default_value: 10000)\n    .flag("),
        "{result}"
    );
    assert_idempotent(source);
}

#[test]
fn generic_static_receivers_inside_parentheses_keep_type_spacing() {
    let source = "(Vector < float >.load(a, i) + Vector < float >.load(b, i)).store(c, i);";
    assert_format(
        source,
        "(Vector<float>.load(a, i) + Vector<float>.load(b, i)).store(c, i);\n",
    );
    assert_idempotent(source);
    assert_format(
        "consume(Vector < float >.load(a, i));",
        "consume(Vector<float>.load(a, i));\n",
    );
}

#[test]
fn independent_while_loops_never_join_like_do_while() {
    let source = "fun f(){while i + lanes <= n {i = i + lanes;} while i < n {i = i + 1;}}";
    let result = format(source);
    assert!(result.contains("    }\n    while i < n {"), "{result}");
    assert!(!result.contains("} while"), "{result}");
    assert_format(
        "while ready {} while pending {}",
        "while ready {}\nwhile pending {}\n",
    );
    assert_format("do {} while (ready);", "do {} while (ready);\n");
    assert_idempotent(source);
}

#[test]
fn generic_arrays_in_parameter_types_keep_type_spacing() {
    assert_format(
        "fun f(values: List < int >[]) {}",
        "fun f(values: List<int>[]) {}\n",
    );
}

#[test]
fn generic_marker_bounds_are_types_instead_of_comparisons() {
    assert_format(
        "public struct Vector < T: unmanaged > {}",
        "public struct Vector<T: unmanaged> {}\n",
    );
    assert_format(
        "shared public class Sender < T: shared > {}",
        "shared public class Sender<T: shared> {}\n",
    );
    assert_format(
        "public class Weak < T: class > {}",
        "public class Weak<T: class> {}\n",
    );
}

#[test]
fn opaque_dsl_payloads_preserve_all_bytes_including_adjacency_and_spaces() {
    let payload =
        "{\n        <div class=\"hero\"><h1>{title}</h1><p>  Welcome  </p></div>  \n    }";
    let source = format!("fun page(title:string):string{{return html {payload};}}");
    let result = format(&source);
    assert!(result.contains(&format!("html {payload};")), "{result}");
    assert_idempotent(&source);
}

#[test]
fn type_members_with_bodies_attributes_or_docs_are_separated() {
    let source = "@json\nclass Person {\n    @min_length(2)\n    @max_length(80)\n    public name: string;\n    @minimum(0.0)\n    @maximum(150.0)\n    @default_json(\"18\")\n    public age: int;\n    public constructor(name: string, age: int) {\n        this.name = name;\n        this.age = age;\n    }\n    public fun greet(): string { return name; }\n}\n";
    assert_format(
        source,
        "@json\nclass Person {\n    @min_length(2)\n    @max_length(80)\n    public name: string;\n\n    @minimum(0.0)\n    @maximum(150.0)\n    @default_json(\"18\")\n    public age: int;\n\n    public constructor(name: string, age: int) {\n        this.name = name;\n        this.age = age;\n    }\n\n    public fun greet(): string {\n        return name;\n    }\n}\n",
    );
    assert_idempotent(source);
}

#[test]
fn plain_fields_stay_grouped_and_function_bodies_are_untouched() {
    assert_format(
        "class Point {\n    public x: int;\n    public y: int;\n    public fun f() {}\n}\nfun g() {\n    let a = 1;\n    let b = 2;\n}\n",
        "class Point {\n    public x: int;\n    public y: int;\n\n    public fun f() {}\n}\n\nfun g() {\n    let a = 1;\n    let b = 2;\n}\n",
    );
    assert_format(
        "interface Shape {\n    fun area(): double;\n    fun name(): string;\n}\n",
        "interface Shape {\n    fun area(): double;\n    fun name(): string;\n}\n",
    );
}

#[test]
fn opaque_payloads_in_statement_fragments_are_preserved() {
    let source = "System.println(\n    quote { Hello generators }\n);\n";
    assert_format(source, source);
}

#[test]
fn weak_local_spacing() {
    let source = "fun f():void{let weak observer=value;const weak other=value;}";
    let formatted = format(source);
    assert!(formatted.contains("let weak observer = value;"));
    assert!(formatted.contains("const weak other = value;"));
    assert_idempotent(source);
}

#[test]
fn ranges_bind_tightly_to_their_bounds() {
    for (source, expected) in [
        ("let r = 0 .. 8;", "let r = 0..8;\n"),
        ("let r = -1 .. n - 1;", "let r = -1..n - 1;\n"),
        ("let r = 1 .. -2;", "let r = 1..-2;\n"),
        (
            "let xs = (0 .. 8).to_list();",
            "let xs = (0..8).to_list();\n",
        ),
        ("let r = 0.5 .. 2.5;", "let r = 0.5..2.5;\n"),
    ] {
        assert_format(source, expected);
        assert_idempotent(source);
    }
    let pattern = format("switch (n) { 0 .. 8 => 1, _ => 0 }");
    assert!(pattern.contains("0..8 => 1"), "{pattern}");
}

#[test]
fn tight_ranges_preserve_comments_and_separate_dot_tokens() {
    for source in [
        "let r = 0 .. .. 8;",
        "let r = 0 . .. 8;",
        "let r = 0 .. . 8;",
        "let r = 0 /* start */ .. /* end */ 8;",
    ] {
        assert!(crate::try_format(source).is_some(), "{source}");
        assert_idempotent(source);
    }
}
