//! Debug harness: `cargo run -p dream-format --example fmt [--check] <file>`
//! Prints the formatted document, or exits 1 with `needs formatting` under `--check`.

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (check, path) = match args.as_slice() {
        [c, p] if c == "--check" => (true, p),
        [p] => (false, p),
        _ => {
            eprintln!("usage: fmt [--check] <file.dream>");
            std::process::exit(2);
        }
    };
    let source = std::fs::read_to_string(path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(2);
    });
    let formatted = dream_format::format(&source);
    if check {
        if formatted != source {
            println!("{path} needs formatting");
            std::process::exit(1);
        }
    } else {
        print!("{}", formatted);
        let again = dream_format::format(&formatted);
        if again != formatted {
            eprintln!("WARNING: formatting is not idempotent for {path}");
            std::process::exit(3);
        }
    }
}
