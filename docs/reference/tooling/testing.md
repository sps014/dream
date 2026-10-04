# Test a project

Tests check that a small part of your program behaves as intended. Put test files in your project's `tests/` folder and mark test functions with `@test`.

```dream
import system.testing;

@test
fun addition() {
    Assert.eq(2 + 3, 5);
}
```

Run the tests from your project folder:

```sh
dreamer test
dreamer test --filter addition
dreamer test --release
```

`--filter` selects test names containing that text. In a workspace, use `-p name` to select a package. Test runs also install development dependencies.

See [Testing APIs](../stdlib/testing.md) for assertion methods and [Workspaces](workspaces.md) for package selection.
