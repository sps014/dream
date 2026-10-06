# Package a CLI program

`dreamer pack` builds a runnable package and writes a relocatable executable to
`target/pack/<os>-<arch>/<name>-<os>-<arch>` (`.exe` on Windows).
Only host libraries selected by live calls are copied beside the executable.
The retained capabilities are core, Unicode, crypto, process, and timezone;
programs without host calls require no host libraries.

```sh
dreamer pack
dreamer pack -O2
dreamer pack --target linux-arm64
dreamer pack --target all
```

Keep the executable and its adjacent libraries together when sharing or moving
the output folder. Target machines need a compatible OS but do not need the
Dream compiler or toolchain. Packaging requires a bin package. Browser and Node
output uses WebAssembly; mobile library packaging has a separate workflow.
