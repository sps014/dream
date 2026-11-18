# Afterglow music player with Raven

Afterglow is a desktop music player with frosted glass panels and album art drawn by Raven. Select **Superhero** by Alex Hagen to play the NCS release, or choose one of three locally synthesized tracks. The transport can pause, skip, seek, and repeat the current track. The library panel controls volume.

Superhero needs an internet connection. The player downloads the MP3 into memory before playback so seeking works throughout the song. A failed load appears in the library panel and keeps the previous track available.

Song: Alex Hagen - Superhero

Music provided by NoCopyrightSounds

Free Download/Stream: https://ncs.io/superheroalexhagenrework

Download and extract the [Raven 0.2.3 shared library archive](https://github.com/oscar-soirey/Raven/releases/tag/v0.2.3) for your desktop platform. Choose the universal macOS, Linux x64, or MSVC Windows x64 archive. Raven uses Vulkan. On macOS install MoltenVK with `brew install molten-vk`. On Linux and Windows use a Vulkan loader and a driver for your GPU. See [Raven's platform setup](https://github.com/oscar-soirey/Raven/blob/v0.2.3/README.md).

## macOS and Linux

From the Dream repository, set `raven` to the absolute path of the extracted archive.

```sh
cd sample/raven
raven=/path/to/extracted/raven
mkdir -p native
case "$(uname -s)" in
  Darwin) cp -P "$raven"/lib/libraven*.dylib native/ ;;
  Linux)  cp -P "$raven"/lib/libraven.so* native/ ;;
esac
CPATH="$raven/include" dreamer run
```

## Windows

From a PowerShell developer terminal, set `$raven` to the absolute path of the extracted MSVC archive.

```powershell
cd sample/raven
$raven = 'C:\path\to\extracted\raven'
New-Item -ItemType Directory -Force native | Out-Null
Copy-Item "$raven/lib/raven.lib" native/
Copy-Item "$raven/bin/raven.dll" native/
$env:CPATH = "$raven/include"
$env:PATH = "$PWD/native;$env:PATH"
dreamer run
```

## How it works

Raven loads the glass materials, controls, and layout from `player.xml`. Keep the working directory at `sample/raven` so it can find that file. Each `cimport` reads one Raven API header and links the same shared library. Dream generates the function, constant, and callback bindings during the build.

The XML names the actions that controls send to Dream. Raven calls a generated C callback to queue each action on the UI thread. The player consumes those actions between frames and uses Dream's native audio API for playback. The covers use Raven's procedural effects. `src/tracks.dream` loads the NCS track from its audio server and synthesizes the other three tracks.
