# Iterable and yield helpers

No import is needed.

Read [Yield and lazy sequences](../language/iterators.md) for sequence creation, cursor access, and cleanup. `Iterable<T>` creates synchronous cursors; `AsyncIterable<T>` creates cursors whose advancement may await. The yield channel, cursor, signal, and lease classes support compiler-generated yield functions.

- [Interfaces, signals, channels, and synchronous cursors](core-iterable-1.md)
- [Sequence factories, async cursors, and cleanup leases](core-iterable-2.md)
