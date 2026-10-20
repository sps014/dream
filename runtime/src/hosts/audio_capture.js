import { audioPlayers } from "./audio.js";

const captureProcessor = `class DreamAudioCapture extends AudioWorkletProcessor {
  constructor(options) {
    super();
    this.frames = options.processorOptions.position;
    this.pool = [];
    this.lost = false;
    this.port.onmessage = event => {
      const samples = event.data;
      this.pool.push({ message: { samples, position: 0, lost: false }, transfer: [samples.buffer] });
    };
  }
  process(inputs, outputs) {
    const input = inputs[0], output = outputs[0];
    for (let channel = 0; channel < output.length; ++channel) {
      if (input[channel]) output[channel].set(input[channel]);
      else output[channel].fill(0);
    }
    const count = output[0].length;
    const item = this.pool.pop();
    if (item) {
      const samples = item.message.samples;
      for (let i = 0; i < count; ++i) {
        samples[i * 2] = output[0][i];
        samples[i * 2 + 1] = output[1][i];
      }
      item.message.position = this.frames;
      item.message.lost = this.lost;
      this.port.postMessage(item.message, item.transfer);
      this.lost = false;
    } else this.lost = true;
    this.frames += count;
    return true;
  }
}
registerProcessor("dream-audio-capture", DreamAudioCapture);`;

const captureModules = new WeakMap();
function installCapture(context) {
  let pending = captureModules.get(context);
  if (!pending) {
    const url = URL.createObjectURL(new Blob([captureProcessor], { type: "text/javascript" }));
    pending = context.audioWorklet.addModule(url).finally(() => URL.revokeObjectURL(url));
    captureModules.set(context, pending);
  }
  return pending;
}

function createCapture() {
  return {
    ring: new Float32Array(32768 * 2), end: 0, available: 0,
    lastRead: 0, lost: false, node: null, generation: 0, rate: 48000,
    disconnect() {
      ++this.generation;
      if (this.node) {
        this.node.port.onmessage = null;
        this.node.port.close();
        this.node.disconnect();
        this.node = null;
      }
    },
    close() { this.disconnect(); },
    connect(player) {
      this.disconnect();
      const generation = this.generation;
      const context = player.output.context;
      this.rate = context.sampleRate;
      installCapture(context).then(() => {
        if (generation !== this.generation || player.closed) return;
        const node = new AudioWorkletNode(context, "dream-audio-capture", {
          numberOfInputs: 1, numberOfOutputs: 1, outputChannelCount: [2],
          channelCount: 2, channelCountMode: "explicit",
          processorOptions: { position: this.end },
        });
        this.node = node;
        node.port.onmessage = event => {
          const { samples, position, lost } = event.data;
          if (lost || position !== this.end) {
            this.available = 0;
            this.lost = true;
          }
          const count = samples.length / 2;
          for (let i = 0; i < count; ++i) {
            const slot = ((position + i) % 32768) * 2;
            this.ring[slot] = samples[i * 2];
            this.ring[slot + 1] = samples[i * 2 + 1];
          }
          this.end = position + count;
          this.available = Math.min(32768, this.available + count);
          node.port.postMessage(samples, [samples.buffer]);
        };
        for (let i = 0; i < 8; ++i) {
          const samples = new Float32Array(256);
          node.port.postMessage(samples, [samples.buffer]);
        }
        player.gain.disconnect();
        player.gain.connect(node);
        node.connect(player.worklet);
      }).catch(() => { this.lost = true; });
    },
    snapshot(frames) {
      const count = Math.min(frames, this.available);
      const start = this.end - count;
      const result = new Float32Array(5 + count * 2);
      result.set([this.rate, 2, start % 16777216, Math.floor(start / 16777216),
        +(this.lost || this.end - this.lastRead > 32768)]);
      for (let i = 0; i < count; ++i) {
        const slot = ((start + i) % 32768) * 2;
        result[5 + i * 2] = this.ring[slot];
        result[6 + i * 2] = this.ring[slot + 1];
      }
      this.lastRead = this.end;
      this.lost = false;
      return result;
    },
  };
}

export function makeAudioCaptureHost() {
  return {
    audioOutputCapture: (id, frames) => {
      const player = audioPlayers.get(id);
      if (!player || player.closed || !Number.isInteger(frames) || frames < 0 || frames > 32768) return [];
      if (!player.capture) {
        player.capture = createCapture();
        if (player.gain) player.capture.connect(player);
      }
      return player.capture.snapshot(frames);
    },
  };
}
