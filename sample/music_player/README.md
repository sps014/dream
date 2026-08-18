# Dream music player (browser-only)

A playable music player — playlist, play/pause, prev/next, seek bar, volume, live time display —
written entirely in [`music_player.dream`](music_player.dream). Every DOM query, `<audio>`
element, event listener, and property get/set is ordinary Dream code compiled to WebAssembly via
the dynamic [`js` interop type](../../docs/reference/language/js-type.md); there is **no hand-written
JavaScript logic** anywhere in this sample. Prefer the tree-shaken host emitted next to the `.wasm`
(`target/web/music_player.web.runtime.js`); the page falls back to [`runtime/dream.js`](../../runtime/dream.js)
if that file is missing.

## Build the artifacts first

The compiled `music_player.wasm`, `music_player.wat`, and `music_player.abi.json` are
**git-ignored**, so a fresh checkout has none. Build them before running:

```sh
# from the repository root
cargo run -- --release --runtime --web -Oz sample/music_player/music_player.dream
```

## Run in the browser

`music_player.html` prefers `./target/web/music_player.web.runtime.js` (from `--runtime --web`). Serve this
folder, or the repository root:

```sh
# from the repository root
npx serve sample/music_player
# then open http://localhost:3000/music_player.html
```

If the selective runtime is missing, the page falls back to `../../runtime/dream.js`, which
requires serving the repository root (`npx serve .`).

If you later add [`WebWorker`](../../docs/reference/language/webworkers.md) calls to a browser sample, the
host page needs [Cross-Origin Isolation](https://developer.mozilla.org/en-US/docs/Web/API/crossOriginIsolated)
headers so Dream can allocate a shared `WebAssembly.Memory` (`SharedArrayBuffer`). With a static
file server such as `npx serve`, add response headers like
`Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp` (see
the WebWorkers doc). This music player does not spawn workers today, so the default serve command
above is enough.

## How it works

- `js.global.document` / `.getElementById(...)` reach every DOM node the player touches.
- `document.createElement("audio")` builds the `<audio>` element — dynamic `js` calls have no
  `new`, so this sidesteps needing `new Audio(...)`.
- Dream functions like `on_play_pause_click(ev: js): void` are passed straight to
  `addEventListener`; the runtime wraps them as JS callables automatically (see
  [Callbacks](../../docs/reference/language/callbacks.md)).
- Because those callbacks can't capture a closure, the playlist, current track index, and every
  DOM handle live in top-level `let`s (see [Variables](../../docs/reference/language/variables.md)),
  shared across all handlers in the file.
- The playlist opens with four freely-licensed [NoCopyrightSounds](https://ncs.io/) (NCS) releases
  mirrored on the Internet Archive, followed by the [SoundHelix](https://www.soundhelix.com/)
  sample MP3s.
