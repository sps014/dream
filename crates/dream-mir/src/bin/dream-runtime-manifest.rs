//! Prints the C runtime module catalog as JSON for `scripts/build-runtime.sh`.

fn main() {
    print!("{}", dream_mir::runtime::manifest_json());
}
