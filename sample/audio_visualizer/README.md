# Audio visualizer

The same Dream program plays audio and draws a waveform and FFT spectrum on native
Windows/macOS/Linux and browser WASM. With no source it creates a quiet 440 Hz tone.

```powershell
cargo run -- run sample/audio_visualizer/main.dream
cargo run -- --web sample/audio_visualizer/main.dream
python -m http.server 8000
```

Open `http://localhost:8000/sample/audio_visualizer/`, select a local file, or enter an
HTTP URL whose server permits CORS. The browser requires user interaction before audio.
Native users can drop a local file onto the window; files and URLs stream, so long
tracks start quickly and use bounded memory.

Space toggles play/pause, left/right seek five seconds, up/down adjust volume,
M toggles mute, D cycles permitted output devices, and Escape closes. P requests
the browser output picker; native applications use the enumerated list instead.
The window title shows peak/RMS. Capture taps this player's rendered stereo PCM
after gain and mute, before it mixes with other players.
