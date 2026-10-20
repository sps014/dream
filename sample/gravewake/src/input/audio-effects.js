// Original layered foley and weapon transients; no sampled recordings.
export function createEffects(context, voices, destination, duck) {
  let serial = 0;
  const noise = options => voices.noise(destination, options);
  const tone = options => voices.tone(destination, options);
  const play = (kind, gain = 1, pan = 0) => {
    gain = Math.max(0, Math.min(1, gain));
    pan = Math.max(-1, Math.min(1, pan));
    const t = context.currentTime + .003;
    const variation = 1 + Math.sin(++serial * 2.39996) * .035;
    const n = (delay, duration, level, frequency, filter = 'lowpass', spread = 0) =>
      noise({time: t + delay, duration, gain: level * gain, frequency: frequency * variation,
        filter, pan: Math.max(-1, Math.min(1, pan + spread))});
    const body = (frequency, duration, level) => tone({time: t, duration,
      frequency: frequency * variation, endFrequency: frequency * .5,
      gain: level * gain, pan, sustain: .05});
    if (kind <= 2) {
      duck();
      const weight = [1, 1.55, 1.8][kind];
      // A short pressure impulse, dry crack, mechanical action and quieter outdoor tail.
      n(0, .022, .82, 3400, 'highpass');
      n(.003, .085 * weight, .75, 950, 'bandpass');
      body(kind === 0 ? 105 : 76, .13 * weight, .45);
      n(.025, .10 * weight, .20, 2600, 'lowpass');
      n(.062, .018, .16, 1800, 'bandpass');
      if (kind === 1) n(.012, .18, .32, 450, 'lowpass');
      if (kind === 2) {
        n(.095, .23, .14, 1700, 'bandpass', -.35);
        n(.17, .31, .08, 1000, 'lowpass', .4);
        n(.54, .035, .13, 3200, 'bandpass');
      }
    } else if (kind === 3) {
      duck();
      body(62, .65, .65);
      n(0, .055, .8, 1200);
      n(.018, 1.15, .8, 350);
      n(.09, .48, .20, 2400, 'bandpass', -.25);
      n(.21, .7, .12, 1600, 'bandpass', .25);
    } else if (kind === 4) {
      body(92, .07, .34);
      n(0, .055, .43, 480, 'bandpass');
      n(.012, .035, .16, 2100, 'bandpass');
    } else if (kind === 5) {
      body(80, .065, .20);
      n(0, .09, .25, 1200, 'bandpass');
      n(.035, .15, .16, 3900, 'highpass');
      n(.09, .05, .08, 1900, 'bandpass');
    } else if (kind === 6) {
      const pitch = 58 * variation;
      for (const [frequency, level] of [[pitch, .14], [pitch * 1.013, .10]]) {
        tone({time: t, duration: .55, frequency, endFrequency: frequency * .88,
          type: 'sawtooth', cutoff: 420, filterEnd: 160, gain: level * gain,
          attack: .035, sustain: .35, pan});
      }
      n(.02, .5, .28, 650, 'bandpass');
    } else if (kind === 7) {
      body(115, .14, .32);
      n(0, .22, .4, 700, 'bandpass');
    } else if (kind === 9) {
      n(0, .09, .45, 2600, 'bandpass');
      n(.04, .34, .28, 1800);
      n(.14, .19, .15, 3300, 'bandpass', .2);
    } else {
      for (const [delay, frequency, level] of [[0, 2200, .35], [.13, 1200, .22], [.36, 2800, .18]]) {
        n(delay, .024, level, frequency, 'bandpass');
      }
      n(.06, .13, .13, 950, 'bandpass');
    }
  };
  return {play};
}
