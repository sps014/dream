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
    assert!(out.contains("@json enum Shape {"), "got:\n{}", out);
    assert!(out.contains("}\n\nenum Other {"), "got:\n{}", out);
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
    ];
    for input in fixtures {
        assert_idempotent(input);
    }
}
