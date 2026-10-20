// Shared voice construction for original synthesized sound and score.
export class AudioVoices {
  constructor(context) {
    this.context = context;
    this.active = new Set();
    this.serial = 0;
    const buffer = context.createBuffer(1, context.sampleRate * 4, context.sampleRate);
    const data = buffer.getChannelData(0);
    let seed = 918273;
    for (let i = 0; i < data.length; i++) {
      seed = (Math.imul(seed, 1664525) + 1013904223) | 0;
      data[i] = (seed >>> 0) / 2147483648 - 1;
    }
    this.noiseBuffer = buffer;
  }

  envelope(node, time, attack, duration, level, sustain) {
    const gain = node.gain;
    gain.setValueAtTime(0, time);
    gain.linearRampToValueAtTime(level, time + attack);
    gain.exponentialRampToValueAtTime(Math.max(.00001, level * sustain), time + duration * .65);
    gain.exponentialRampToValueAtTime(.00001, time + duration);
  }

  finish(source, nodes, time, duration, offset) {
    this.active.add(source);
    if (offset === undefined) source.start(time);
    else source.start(time, offset);
    source.stop(time + duration + .01);
    source.onended = () => {
      this.active.delete(source);
      source.disconnect();
      for (const node of nodes) node.disconnect();
    };
  }

  noise(destination, {time = this.context.currentTime, duration = .1, gain = .1,
    frequency = 3000, filter = 'lowpass', q = .7, pan = 0, attack = .002,
    sustain = .12, rate = 1} = {}) {
    if (this.active.size >= 96) return;
    const source = this.context.createBufferSource();
    source.buffer = this.noiseBuffer;
    source.playbackRate.value = rate;
    const color = this.context.createBiquadFilter();
    color.type = filter;
    color.frequency.value = frequency;
    color.Q.value = q;
    const envelope = this.context.createGain();
    this.envelope(envelope, time, attack, duration, gain, sustain);
    const panner = this.context.createStereoPanner();
    panner.pan.value = pan;
    source.connect(color).connect(envelope).connect(panner).connect(destination);
    const available = Math.max(0, 4 - duration * rate - .05);
    const offset = (++this.serial * .371) % Math.max(.001, available);
    this.finish(source, [color, envelope, panner], time, duration, offset);
  }

  tone(destination, {time = this.context.currentTime, duration = .15, frequency = 100,
    endFrequency = frequency, gain = .1, type = 'sine', detune = 0, cutoff = 18000,
    filterEnd = cutoff, pan = 0, attack = .003, sustain = .2} = {}) {
    if (this.active.size >= 96) return;
    const source = this.context.createOscillator();
    source.type = type;
    source.detune.value = detune;
    source.frequency.setValueAtTime(frequency, time);
    source.frequency.exponentialRampToValueAtTime(endFrequency, time + Math.min(duration, .16));
    const filter = this.context.createBiquadFilter();
    filter.type = 'lowpass';
    filter.Q.value = .65;
    filter.frequency.setValueAtTime(cutoff, time);
    filter.frequency.exponentialRampToValueAtTime(filterEnd, time + duration);
    const envelope = this.context.createGain();
    this.envelope(envelope, time, attack, duration, gain, sustain);
    const panner = this.context.createStereoPanner();
    panner.pan.value = pan;
    source.connect(filter).connect(envelope).connect(panner).connect(destination);
    this.finish(source, [filter, envelope, panner], time, duration);
  }
}
