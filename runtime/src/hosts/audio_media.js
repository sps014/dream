import { audioPlayers, allocateAudioPlayer, audioFailure } from "./audio.js";

// URL loading, range requests, container indexes and decoder scheduling belong to
// the browser. A private context permits per-player sinks without rerouting peers.
export function makeAudioMediaHost() {
  return {
    audioMediaCreate: src => {
      if (!globalThis.document || !globalThis.AudioContext) return 0;
      const element = document.createElement("audio");
      element.crossOrigin = "anonymous";
      element.preload = "auto";
      const player = { closed: false, gain: null, worklet: null, output: null, capture: null };
      let status = 0, waiting = false, epoch = 0, seeking = false;
      let source, previous = 0;
      let cancelSeek = null;
      element.oncanplay = () => { status = 1; waiting = false; };
      element.onwaiting = () => { waiting = true; };
      element.onplaying = () => { waiting = false; };
      element.onerror = () => { status = -1; cancelSeek?.(); };
      async function graph() {
        if (player.output) return;
        const context = new AudioContext({ sampleRate: 48000 });
        player.output = { context };
        source = context.createMediaElementSource(element);
        player.gain = context.createGain();
        // Capture installs its own worklet between gain and this output node.
        player.worklet = context.createGain();
        source.connect(player.gain);
        player.gain.connect(player.worklet);
        player.worklet.connect(context.destination);
        player.gain.gain.value = gain;
        player.capture?.connect(player);
      }
      let gain = 1, sink = "";
      player.media = {
        async start() {
          const generation = ++epoch;
          try {
            await graph();
            await player.output.context.resume();
            if (player.closed || generation !== epoch) return -6;
            await element.play();
            if (player.closed || generation !== epoch) { element.pause(); return -6; }
            return 1;
          } catch (error) { return audioFailure(error); }
        },
        control(command, value) {
          if (command === 2) { ++epoch; element.pause(); return 1; }
          if (command === 3) return 1; // The asynchronous media seek already positioned it.
          if (command === 4) { gain = value; if (player.gain) player.gain.gain.value = value; return 1; }
          if (command === 5) { element.loop = !!value; return 1; }
          return 0;
        },
        query(field) {
          if (field === 1) return (seeking ? previous : element.currentTime) * 48000;
          if (field === 2) return +element.ended;
          if (field === 3) return +(player.output?.context.state === "closed");
          if (field === 4) return element.readyState >= 3 ? 48000 : 0;
          if (field === 5) return +waiting;
          if (field === 6) return Number.isFinite(element.duration) ? element.duration : -1;
          if (field === 7) return status;
          return +!element.paused;
        },
        async seek(time) {
          if (seeking) return -7;
          const ranges = element.seekable;
          let supported = false;
          for (let i = 0; i < ranges.length; ++i) {
            if (time >= ranges.start(i) && time <= ranges.end(i)) supported = true;
          }
          if (!supported) return -1;
          if (Math.abs(element.currentTime - time) < 0.000001) return 1;
          previous = element.currentTime;
          seeking = true;
          return await new Promise(resolve => {
            let timer;
            const finish = result => {
              clearTimeout(timer);
              element.removeEventListener("seeked", complete);
              cancelSeek = null;
              if (result !== 1 && !player.closed) {
                try { element.currentTime = previous; } catch (_) {}
              }
              seeking = false;
              resolve(result);
            };
            const complete = () => finish(Math.abs(element.currentTime - time) < 0.1 ? 1 : 0);
            cancelSeek = () => finish(-6);
            element.addEventListener("seeked", complete);
            timer = setTimeout(() => finish(0), 15000);
            try { element.currentTime = time; } catch (error) { finish(audioFailure(error)); }
          });
        },
        async sink(id) {
          if (id === sink) return 1;
          try {
            await graph();
            if (!player.output.context.setSinkId) return -1;
            await player.output.context.setSinkId(id);
            sink = id;
            return 1;
          } catch (error) { return audioFailure(error); }
        },
        close() {
          player.closed = true;
          ++epoch;
          cancelSeek?.();
          element.pause();
          element.removeAttribute("src");
          element.load();
          element.oncanplay = element.onloadedmetadata = element.onwaiting = element.onplaying = element.onerror = null;
          player.capture?.close();
          source?.disconnect();
          player.gain?.disconnect();
          player.worklet?.disconnect();
          player.output?.context.close().catch(() => {});
        },
      };
      const id = allocateAudioPlayer(player);
      element.src = src;
      element.load();
      return id;
    },
    audioMediaSeek: async (id, time) => {
      const player = audioPlayers.get(id);
      return player?.media && !player.closed ? player.media.seek(time) : -6;
    },
  };
}
