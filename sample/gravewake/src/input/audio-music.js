// Original 108 BPM F# minor score. Layers change; tempo and peak level stay bounded.
const CHORDS = [
  {bass: 30, notes: [54, 57, 61, 68]},
  {bass: 26, notes: [50, 54, 57, 64]},
  {bass: 33, notes: [52, 57, 61, 64]},
  {bass: 28, notes: [52, 56, 59, 66]},
];
const frequency = midi => 440 * 2 ** ((midi - 69) / 12);

export class SynthwaveScore {
  constructor(context, voices, destination) {
    this.context = context;
    this.voices = voices;
    this.sixteenth = 60 / 108 / 4;
    this.step = 0;
    this.next = context.currentTime + .06;
    this.intensity = .08;
    this.output = context.createGain();
    this.output.gain.value = .5;
    this.output.connect(destination);
    this.ducking = context.createGain();
    this.ducking.connect(this.output);
    this.layers = Array.from({length: 5}, () => {
      const gain = context.createGain();
      gain.gain.value = 0;
      gain.connect(this.ducking);
      return gain;
    });
    this.echo = context.createDelay(1);
    this.echo.delayTime.value = this.sixteenth * 3;
    const feedback = context.createGain();
    feedback.gain.value = .22;
    const color = context.createBiquadFilter();
    color.type = 'lowpass';
    color.frequency.value = 2400;
    this.echo.connect(color).connect(feedback).connect(this.echo);
    const wet = context.createGain();
    wet.gain.value = .20;
    color.connect(wet).connect(this.ducking);
    this.layers[3].connect(this.echo);
    this.layers[4].connect(this.echo);
    this.setCombat({wave: 1, alive: 0, intermission: false, state: 0});
  }

  setCombat({wave, alive, intermission, state}) {
    this.intensity = Math.max(0, Math.min(1, (wave - 1) / 7 + alive / 48));
    const i = this.intensity;
    const calm = intermission ? .55 : 1;
    const volume = state === 0 ? calm : .2;
    // More rhythmic detail supplies tension without continually raising the master volume.
    const levels = [.30 - i * .06, .42, .36, .10 + i * .24, Math.max(0, i - .35) * .20];
    levels.forEach((level, index) => {
      this.layers[index].gain.setTargetAtTime(level * volume, this.context.currentTime, 1.6);
    });
  }

  duck() {
    const t = this.context.currentTime;
    const gain = this.ducking.gain;
    gain.cancelScheduledValues(t);
    gain.setValueAtTime(.58, t);
    gain.setTargetAtTime(1, t + .045, .11);
  }

  schedule(step, t) {
    const s = step % 16;
    const bar = Math.floor(step / 16) % 8;
    const chord = CHORDS[Math.floor(bar / 2)];
    const tone = (layer, options) => this.voices.tone(this.layers[layer], {time: t, ...options});
    const noise = options => this.voices.noise(this.layers[2], {time: t, ...options});
    if (s === 0 && bar % 2 === 0) {
      chord.notes.slice(0, 3).forEach((note, index) => {
        for (const detune of [-7, 7]) tone(0, {
          duration: this.sixteenth * 33, frequency: frequency(note),
          type: 'sawtooth', detune, gain: .055, cutoff: 900 + index * 120,
          filterEnd: 580, attack: .35, sustain: .75, pan: detune / 12,
        });
      });
    }
    if (s % 2 === 0) {
      const octave = s === 6 || s === 14 ? 12 : 0;
      tone(1, {duration: this.sixteenth * 1.75, frequency: frequency(chord.bass + octave),
        type: 'sawtooth', gain: .26, cutoff: 1100, filterEnd: 140, sustain: .35});
      tone(1, {duration: this.sixteenth * 1.75, frequency: frequency(chord.bass),
        type: 'sine', gain: .16, sustain: .3});
    }
    if (s % 4 === 0 || (s === 14 && bar === 7)) {
      tone(2, {duration: .19, frequency: 130, endFrequency: 43, gain: .70, sustain: .03});
      noise({duration: .015, gain: .12, frequency: 2200, filter: 'highpass'});
    }
    if (s === 4 || s === 12) {
      noise({duration: .13, gain: .31, frequency: 2200, filter: 'highpass'});
      tone(2, {duration: .08, frequency: 175, endFrequency: 145, gain: .16, sustain: .05});
      // A pair of quieter echoes gives the snare space without a long wash.
      this.voices.noise(this.layers[2], {time: t + .035, duration: .09, gain: .08,
        frequency: 3300, filter: 'bandpass', pan: .35});
    }
    if (s % 2 === 0 || this.intensity > .5) {
      noise({duration: s === 14 ? .09 : .028, gain: s % 4 === 2 ? .075 : .045,
        frequency: 7400, filter: 'highpass', pan: s % 4 === 0 ? -.3 : .3});
    }
    if (s % 2 === 0 || this.intensity > .25) {
      const order = [0, 2, 1, 3, 2, 1, 3, 2];
      const note = chord.notes[order[s % 8]] + 12;
      tone(3, {duration: this.sixteenth * 1.8, frequency: frequency(note),
        type: 'triangle', gain: .14, cutoff: 3000, filterEnd: 900,
        sustain: .15, pan: Math.sin(step * .7) * .45});
    }
    if (bar % 2 === 1 && [2, 6, 10, 14].includes(s)) {
      const melody = [73, 76, 73, 71, 69, 73, 76, 80];
      const note = melody[(bar * 2 + Math.floor(s / 4)) % melody.length];
      for (const detune of [-5, 5]) tone(4, {
        duration: this.sixteenth * 3.2, frequency: frequency(note),
        type: 'sawtooth', detune, gain: .07, cutoff: 1900, filterEnd: 800,
        attack: .012, sustain: .4, pan: detune / 10,
      });
    }
  }

  pump() {
    if (this.context.state !== 'running') return;
    if (this.next < this.context.currentTime) this.next = this.context.currentTime + .025;
    while (this.next < this.context.currentTime + .12) {
      this.schedule(this.step++, this.next);
      this.next += this.sixteenth;
    }
  }

  start() {
    this.pump();
    this.timer = setInterval(() => this.pump(), 25);
  }

  stop() {
    clearInterval(this.timer);
  }
}
