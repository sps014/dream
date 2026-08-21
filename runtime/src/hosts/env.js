/** Default `env` builtins every Dream module imports (mirrors the WASM guest ABI). */

/** Native host parity: `%.6f` with trailing zeros (and dot) trimmed. */
function formatFloat(v) {
  if (!Number.isFinite(v)) return String(v);
  let text = v.toFixed(6);
  if (text.includes(".")) {
    text = text.replace(/0+$/, "").replace(/\.$/, "");
  }
  return text === "-0" ? "0" : text;
}

/** Native host parity: `%.16g`. */
function formatDouble(v) {
  if (!Number.isFinite(v)) return String(v);
  return String(Number(v.toPrecision(16)));
}

function defaultEnv(getInstance, options) {
  const writeOut = options.stdout || ((s) => (typeof process !== "undefined" ? process.stdout.write(s) : console.log(s)));
  const writeLine = options.stdout
    ? (s) => options.stdout(s + "\n")
    : (s) => console.log(s);

  return {
    print_string: (ptr) => writeOut(getInstance().readString(ptr)),
    println: (ptr) => writeLine(getInstance().readString(ptr)),
    print_int: (v) => writeOut(String(v)),
    print_float: (v) => writeOut(formatFloat(v)),
    print_double: (v) => writeOut(formatDouble(v)),
    print_char: (v) => writeOut(String.fromCharCode(v)),
    sin: Math.sin,
    cos: Math.cos,
    tan: Math.tan,
    asin: Math.asin,
    acos: Math.acos,
    atan: Math.atan,
    atan2: Math.atan2,
    abs: Math.abs,
    sqrt: Math.sqrt,
    pow: Math.pow,
    log: Math.log,
    log10: Math.log10,
    exp: Math.exp,
    hypot: Math.hypot,
    floor: Math.floor,
    ceil: Math.ceil,
    round: Math.round,
  };
}
