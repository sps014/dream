# Color Studio with raygui

Color Studio is a desktop GUI with a live color preview. Drag inside the picker or along the hue bar to change the swatch. **Reset color** restores the starting color. Press Escape or close the window to quit.

Use the [raylib 6.2 desktop release](https://github.com/raysan5/raylib/releases/tag/6.2) for your OS and CPU, and the [raygui 5.0 header](https://github.com/raysan5/raygui/releases/tag/5.0). Extract the raylib release before running the commands below. Run the sample on a desktop with a graphical session.

## macOS and Linux

From the root of the Dream repository, set `raylib` to the absolute path of the extracted release.

```sh
cd sample/raygui
raylib=/path/to/extracted/raylib
mkdir -p target/deps/include
curl -fLsS https://raw.githubusercontent.com/raysan5/raygui/5.0/src/raygui.h \
  -o target/deps/include/raygui.h
cp "$raylib/include/raylib.h" target/deps/include/

case "$(uname -s)" in
  Darwin) cp -P "$raylib"/lib/libraylib*.dylib native/ ;;
  Linux)  cp -P "$raylib"/lib/libraylib.so* native/ ;;
esac

CPATH="$PWD/target/deps/include" dreamer run
```

## Windows

Use the MSVC build of raylib. From a PowerShell developer terminal, set `$raylib` to the absolute path of the extracted release.

```powershell
cd sample/raygui
$raylib = 'C:\path\to\extracted\raylib'
New-Item -ItemType Directory -Force target/deps/include | Out-Null
Invoke-WebRequest https://raw.githubusercontent.com/raysan5/raygui/5.0/src/raygui.h `
  -OutFile target/deps/include/raygui.h
Copy-Item "$raylib/include/raylib.h" target/deps/include/
Copy-Item "$raylib/lib/raylibdll.lib" native/raylib.lib
Copy-Item "$raylib/lib/raylib.dll" native/
$env:CPATH = "$PWD/target/deps/include"
$env:PATH = "$PWD/native;$env:PATH"
dreamer run
```

## How the bindings work

`cimport "raylib.h" from "raylib" as rl` finds the installed raylib header through Clang’s include search path and links the shared library. `cimport "raygui.h" as gui` binds to the header in the package’s native include path. The two-line `native/raygui.c` compiles raygui’s header-only implementation.

Dream generates the function bindings, value structs, and style constants at build time. The color picker receives a pointer to a writable color. Dream owns that allocation, reads it to draw the preview, and frees it and the UTF-8 C strings when the window closes.
