// Output operations are independent of encoded-format decoding and spectrum analysis.
export const audioPlayers = new Map();
const audioOutputs = new Map();
const audioOutputPending = new Map();
export function allocateAudioPlayer(player) {
  const id = nextAudioPlayer++;
  audioPlayers.set(id, player);
  return id;
}
let nextAudioPlayer = 1;
export function audioFailure(error) {
  if (error?.name === "NotAllowedError") return -2;
  if (error?.name === "SecurityError") return -3;
  if (error?.name === "NotFoundError") return -5;
  if (error?.name === "AbortError") return -6;
  if (error?.name === "NotSupportedError") return -1;
  return 0;
}

function audioDeviceError(code, message) {
  return JSON.stringify({ error: { code, message } });
}
const audioProcessor = `class DreamAudioOutput extends AudioWorkletProcessor {
  process(inputs, outputs) {
    const input = inputs[0];
    const output = outputs[0];
    for (let channel = 0; channel < output.length; channel++) {
      const samples = input[channel];
      if (samples) output[channel].set(samples);
      else output[channel].fill(0);
    }
    return true;
  }
}
registerProcessor("dream-audio-output", DreamAudioOutput);
class DreamAudioStream extends AudioWorkletProcessor {
  constructor(options) {
    super();
    const { channels, rate, capacity } = options.processorOptions;
    this.channels = channels;
    this.step = rate / sampleRate;
    this.capacity = capacity;
    this.ring = new Float32Array(capacity * channels);
    this.read = 0; this.write = 0; this.fraction = 0; this.consumed = 0; this.epoch = 0;
    this.running = false; this.eos = false; this.ended = false; this.starved = false; this.quanta = 0;
    this.port.onmessage = ({ data }) => {
      if (data.type === "flush") {
        this.epoch = data.epoch; this.consumed = data.consumed;
        this.read = 0; this.write = 0; this.fraction = 0;
        this.eos = false; this.ended = false; this.starved = false;
        return;
      }
      if (data.type === "run") { this.running = data.running; return; }
      if (data.epoch !== this.epoch) return;
      if (data.type === "eos") this.eos = true;
      else if (data.type === "data") {
        const samples = data.samples, frames = samples.length / this.channels;
        for (let frame = 0; frame < frames; frame++) {
          const slot = ((this.write + frame) % this.capacity) * this.channels;
          for (let channel = 0; channel < this.channels; channel++) {
            this.ring[slot + channel] = samples[frame * this.channels + channel];
          }
        }
        this.write += frames;
      }
    };
  }
  process(inputs, outputs) {
    const output = outputs[0], frames = output[0].length;
    const before = this.starved || this.ended;
    for (let i = 0; i < frames; i++) {
      const available = this.write - this.read;
      if (!this.running || this.ended || available === 0 || (available === 1 && !this.eos)) {
        if (this.running && !this.ended) {
          if (available === 0 && this.eos) this.ended = true;
          else this.starved = true;
        }
        for (let channel = 0; channel < output.length; channel++) output[channel][i] = 0;
        continue;
      }
      this.starved = false;
      const a = (this.read % this.capacity) * this.channels;
      const b = available > 1 ? ((this.read + 1) % this.capacity) * this.channels : a;
      for (let channel = 0; channel < output.length; channel++) {
        const source = Math.min(channel, this.channels - 1);
        output[channel][i] = this.ring[a + source] + (this.ring[b + source] - this.ring[a + source]) * this.fraction;
      }
      this.fraction += this.step;
      const whole = Math.min(Math.floor(this.fraction), this.write - this.read);
      this.fraction -= Math.floor(this.fraction);
      this.read += whole;
      this.consumed += whole;
    }
    if (++this.quanta % 8 === 0 || before !== (this.starved || this.ended)) {
      this.port.postMessage({ epoch: this.epoch, consumed: this.consumed, starved: this.starved, ended: this.ended });
    }
    return true;
  }
}
registerProcessor("dream-audio-stream", DreamAudioStream);`;

async function createAudioOutput(id) {
  const Context = globalThis.AudioContext;
  if (!Context) throw new DOMException("audio output is unavailable", "NotSupportedError");
  const context = new Context({ sampleRate: 48000 });
  try {
    if (id) {
      if (!context.setSinkId) throw new DOMException("output selection is unsupported", "NotSupportedError");
      await context.setSinkId(id);
    }
    if (!context.audioWorklet) throw new DOMException("AudioWorklet requires a secure context", "SecurityError");
    const url = URL.createObjectURL(new Blob([audioProcessor], { type: "text/javascript" }));
    try { await context.audioWorklet.addModule(url); }
    finally { URL.revokeObjectURL(url); }
    const output = { context, refs: 0, id, lost: false, onDeviceChange: null };
    if (id && id !== "default" && globalThis.navigator?.mediaDevices?.addEventListener) {
      output.onDeviceChange = async () => {
        try {
          const devices = await navigator.mediaDevices.enumerateDevices();
          if (devices.some(device => device.kind === "audiooutput" && device.deviceId === id)) return;
          output.lost = true;
          for (const player of audioPlayers.values()) {
            if (player.output === output) pauseAudioPlayer(player);
          }
        } catch (_) { /* A failed permission query cannot prove that a device disappeared. */ }
      };
      navigator.mediaDevices.addEventListener("devicechange", output.onDeviceChange);
    }
    audioOutputs.set(id, output);
    return output;
  } catch (error) {
    await context.close();
    throw error;
  }
}

async function acquireAudioOutput(id) {
  let output = audioOutputs.get(id);
  if (!output) {
    let pending = audioOutputPending.get(id);
    if (!pending) {
      pending = createAudioOutput(id).finally(() => audioOutputPending.delete(id));
      audioOutputPending.set(id, pending);
    }
    output = await pending;
  }
  output.refs++;
  return output;
}

function releaseAudioOutput(output) {
  if (!output || --output.refs > 0) return;
  audioOutputs.delete(output.id);
  if (output.onDeviceChange) navigator.mediaDevices.removeEventListener("devicechange", output.onDeviceChange);
  output.context.close().catch(() => {});
}

function audioPosition(player) {
  if (player.stream) return (player.stream.base + player.stream.consumed) / player.rate;
  let position = player.position;
  if (player.playing && player.output) {
    position += player.output.context.currentTime - player.started;
  }
  const duration = player.samples.length / player.channels / player.rate;
  return player.loop && duration > 0 ? position % duration : Math.min(position, duration);
}

function pauseAudioPlayer(player) {
  player.position = audioPosition(player);
  player.playing = false;
  player.stream?.node?.port.postMessage({ type: "run", running: false });
  if (player.source) {
    player.source.onended = null;
    player.source.stop();
    player.source.disconnect();
    player.source = null;
  }
}

function createAudioStream(rate, channels, capacity) {
  return { rate, channels, capacity, epoch: 0, written: 0, consumed: 0, base: 0,
    chunks: [], eos: false, ended: false, starved: false, node: null };
}

function postAudioChunk(stream, chunk, skip) {
  const samples = chunk.data.subarray(skip * stream.channels);
  if (samples.length) stream.node.port.postMessage({ type: "data", epoch: stream.epoch, samples });
}

// A replacement node resumes from the consumed position using retained, unplayed chunks.
function connectAudioStream(player) {
  const stream = player.stream, context = player.output.context;
  if (!stream.node) {
    const node = new AudioWorkletNode(context, "dream-audio-stream", {
      numberOfInputs: 0, numberOfOutputs: 1, outputChannelCount: [player.channels],
      processorOptions: { channels: player.channels, rate: player.rate, capacity: stream.capacity },
    });
    node.port.onmessage = ({ data }) => {
      if (stream.node !== node || data.epoch !== stream.epoch) return;
      stream.consumed = data.consumed;
      stream.starved = data.starved;
      while (stream.chunks.length) {
        const chunk = stream.chunks[0];
        if (chunk.start + chunk.data.length / stream.channels > stream.consumed) break;
        stream.chunks.shift();
      }
      if (data.ended && !stream.ended) {
        stream.ended = true;
        player.ended = true;
        player.playing = false;
      }
    };
    stream.node = node;
    node.port.postMessage({ type: "flush", epoch: stream.epoch, consumed: stream.consumed });
    for (const chunk of stream.chunks) postAudioChunk(stream, chunk, Math.max(0, stream.consumed - chunk.start));
    if (stream.eos) node.port.postMessage({ type: "eos", epoch: stream.epoch });
    node.connect(player.gain);
  }
  player.gain.gain.value = player.volume;
  player.playing = true;
  player.ended = stream.ended;
  stream.node.port.postMessage({ type: "run", running: true });
}

function disconnectAudioStream(stream) {
  if (!stream?.node) return;
  stream.node.port.onmessage = null;
  stream.node.port.close();
  stream.node.disconnect();
  stream.node = null;
}

function connectAudioPlayer(player) {
  const context = player.output.context;
  if (player.stream) {
    if (!player.gain) {
      player.gain = context.createGain();
      player.worklet = new AudioWorkletNode(context, "dream-audio-output", {
        numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [player.channels],
      });
      player.gain.connect(player.worklet);
      player.worklet.connect(context.destination);
      player.capture?.connect(player);
    }
    connectAudioStream(player);
    return;
  }
  if (!player.buffer || player.buffer.sampleRate !== player.rate) {
    player.buffer = context.createBuffer(player.channels,
      player.samples.length / player.channels, player.rate);
    for (let channel = 0; channel < player.channels; channel++) {
      const destination = player.buffer.getChannelData(channel);
      for (let frame = 0; frame < destination.length; frame++) {
        destination[frame] = player.samples[frame * player.channels + channel];
      }
    }
  }
  if (!player.gain) {
    player.gain = context.createGain();
    player.worklet = new AudioWorkletNode(context, "dream-audio-output", {
      numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [player.channels],
    });
    player.gain.connect(player.worklet);
    player.worklet.connect(context.destination);
    player.capture?.connect(player);
  }
  player.gain.gain.value = player.volume;
  const source = context.createBufferSource();
  source.buffer = player.buffer;
  source.loop = player.loop;
  source.connect(player.gain);
  source.onended = () => {
    if (player.source !== source) return;
    player.position = player.samples.length / player.channels / player.rate;
    player.playing = false;
    player.ended = true;
    source.disconnect();
    player.source = null;
  };
  player.started = context.currentTime;
  player.source = source;
  player.playing = true;
  player.ended = false;
  source.start(0, player.position);
}

export function makeAudioHost() {
  return {
    audioOutputCreate: (samples, rate, channels) => {
      if (!globalThis.AudioContext) return -1;
      if (!Number.isInteger(rate) || rate <= 0 ||
          (channels !== 1 && channels !== 2) || !samples.length || samples.length % channels) return 0;
      const id = nextAudioPlayer++;
      audioPlayers.set(id, { samples: Float32Array.from(samples), rate, channels,
        position: 0, started: 0, volume: 1, loop: false, playing: false,
        ended: false, closed: false, startGeneration: 0, sinkPending: false, sink: "", output: null, source: null,
        buffer: null, gain: null, worklet: null, stream: null });
      return id;
    },
    audioOutputStream: (rate, channels, frames) => {
      if (!globalThis.AudioContext) return -1;
      if (!Number.isInteger(rate) || rate <= 0 || rate > 384000 || (channels !== 1 && channels !== 2) ||
          !Number.isInteger(frames) || frames <= 0 || frames > rate * 30) return 0;
      const id = nextAudioPlayer++;
      audioPlayers.set(id, { samples: null, rate, channels,
        position: 0, started: 0, volume: 1, loop: false, playing: false,
        ended: false, closed: false, startGeneration: 0, sinkPending: false, sink: "", output: null, source: null,
        buffer: null, gain: null, worklet: null, stream: createAudioStream(rate, channels, frames) });
      return id;
    },
    audioOutputWrite: (id, samples) => {
      const player = audioPlayers.get(id), stream = player?.stream;
      if (!stream || player.closed || stream.eos) return 0;
      const space = stream.capacity - (stream.written - stream.consumed);
      const frames = Math.min(Math.floor(samples.length / stream.channels), space);
      if (frames <= 0) return 0;
      const chunk = { start: stream.written, data: Float32Array.from(samples.slice(0, frames * stream.channels)) };
      stream.chunks.push(chunk);
      stream.written += frames;
      if (stream.node) postAudioChunk(stream, chunk, 0);
      return frames;
    },
    audioOutputStart: async id => {
      const player = audioPlayers.get(id);
      if (!player || player.closed) return 0;
      if (player.media) return player.media.start();
      const generation = ++player.startGeneration;
      try {
        if (!player.output) {
          const output = await acquireAudioOutput(player.sink);
          if (player.closed || generation !== player.startGeneration) {
            releaseAudioOutput(output);
            return -6;
          }
          player.output = output;
        }
        await player.output.context.resume();
        if (player.closed || generation !== player.startGeneration) return -6;
        if (player.output.lost) return -5;
        if (player.output.context.state !== "running") return -2;
        if (!player.playing) connectAudioPlayer(player);
        return 1;
      } catch (error) { return audioFailure(error); }
    },
    audioOutputControl: (id, command, value) => {
      const player = audioPlayers.get(id);
      if (!player || player.closed || !Number.isFinite(value)) return 0;
      if (player.media) return player.media.control(command, value);
      const stream = player.stream;
      if (stream && (command === 3 || command === 5 || command === 6)) {
        if (command === 3) {
          if (value < 0) return 0;
          stream.epoch++;
          stream.written = 0; stream.consumed = 0; stream.base = value;
          stream.chunks = []; stream.eos = false; stream.ended = false; stream.starved = false;
          player.ended = false;
          stream.node?.port.postMessage({ type: "flush", epoch: stream.epoch, consumed: 0 });
        } else if (command === 6) {
          stream.eos = true;
          stream.node?.port.postMessage({ type: "eos", epoch: stream.epoch });
        } else player.loop = !!value;
        return 1;
      }
      if (command === 2) {
        ++player.startGeneration;
        pauseAudioPlayer(player);
      }
      else if (command === 3) {
        const playing = player.playing;
        pauseAudioPlayer(player);
        player.position = value / player.rate;
        player.ended = false;
        if (playing) connectAudioPlayer(player);
      } else if (command === 4) {
        player.volume = value;
        if (player.gain) player.gain.gain.value = value;
      } else if (command === 5) {
        player.loop = !!value;
        if (player.source) player.source.loop = player.loop;
      } else return 0;
      return 1;
    },
    audioOutputQuery: (id, field) => {
      const player = audioPlayers.get(id);
      if (!player) return 0;
      if (player.media) return player.media.query(field);
      if (field === 1) return audioPosition(player) * player.rate;
      if (field === 2) return +player.ended;
      if (field === 3) return +(player.output?.lost || player.output?.context.state === "closed");
      if (field === 4) return player.stream ? player.stream.written - player.stream.consumed : 0;
      if (field === 5) return +!!(player.stream?.starved && player.playing);
      return +player.playing;
    },
    audioOutputClose: id => {
      const player = audioPlayers.get(id);
      if (!player) return;
      if (player.media) {
        player.media.close();
        audioPlayers.delete(id);
        return;
      }
      player.closed = true;
      pauseAudioPlayer(player);
      disconnectAudioStream(player.stream);
      player.gain?.disconnect();
      player.worklet?.disconnect();
      player.capture?.close();
      releaseAudioOutput(player.output);
      audioPlayers.delete(id);
    },
    audioOutputDevices: async () => {
      if (!globalThis.navigator?.mediaDevices?.enumerateDevices) {
        return audioDeviceError(globalThis.isSecureContext === false ? "ESECURITY" : "ENOTSUP",
          "output enumeration requires browser support and a secure context");
      }
      try {
        const devices = await navigator.mediaDevices.enumerateDevices();
        return JSON.stringify(devices.filter(device => device.kind === "audiooutput").map(device => ({
          id: device.deviceId, name: device.label, is_default: device.deviceId === "default",
        })));
      } catch (error) {
        return audioDeviceError({ NotAllowedError: "EACCES", SecurityError: "ESECURITY" }[error.name] || "EIO", error.message);
      }
    },
    audioOutputRequest: async () => {
      if (!globalThis.navigator?.mediaDevices?.selectAudioOutput) {
        return audioDeviceError(globalThis.isSecureContext === false ? "ESECURITY" : "ENOTSUP",
          "output selection requires browser support and a secure context");
      }
      if (globalThis.navigator.userActivation?.isActive === false) {
        return audioDeviceError("EACTIVATION", "output selection requires a user gesture");
      }
      try {
        const device = await navigator.mediaDevices.selectAudioOutput();
        return JSON.stringify([{ id: device.deviceId, name: device.label, is_default: device.deviceId === "default" }]);
      } catch (error) {
        const code = { NotAllowedError: "EACCES", SecurityError: "ESECURITY",
          NotFoundError: "ENOTFOUND", AbortError: "ECANCELLED", InvalidStateError: "EACTIVATION" }[error.name] || "EIO";
        return audioDeviceError(code, error.message);
      }
    },
    audioOutputSink: async (id, sink) => {
      const player = audioPlayers.get(id);
      if (!player || player.closed) return 0;
      if (player.media) return player.media.sink(sink);
      if (player.sink === sink) return 1;
      if (player.sinkPending) return -7;
      player.sinkPending = true;
      let output;
      try {
        output = await acquireAudioOutput(sink);
        await output.context.resume();
      } catch (error) {
        if (output) releaseAudioOutput(output);
        player.sinkPending = false;
        return audioFailure(error);
      }
      player.sinkPending = false;
      if (player.closed) { releaseAudioOutput(output); return -6; }
      const playing = player.playing;
      pauseAudioPlayer(player);
      disconnectAudioStream(player.stream);
      player.gain?.disconnect();
      player.worklet?.disconnect();
      player.capture?.disconnect();
      releaseAudioOutput(player.output);
      player.output = output;
      player.sink = sink;
      player.gain = null;
      player.worklet = null;
      if (playing) connectAudioPlayer(player);
      return 1;
    },
  };
}
