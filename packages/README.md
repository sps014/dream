# Packages

Dream libraries that ship with this repository but are not embedded in the compiler. Each directory is a `dream.toml` package. A program depends on one by path and imports it by name:

```toml
[dependencies]
unicode = { path = "packages/unicode" }
```

```dream
import unicode;
```

`dreamer install` from this directory (or from a program that depends on a member) links the package into `dream_packages/`.
