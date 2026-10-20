import {AudioVoices} from './audio-voices.js';
import {createEffects} from './audio-effects.js';
import {SynthwaveScore} from './audio-music.js';

export function createGameAudio(document) {
  let context, master, effects, music;
  let muted = false;
  let combat = {wave: 1, alive: 0, intermission: false, state: 0};
  const ensure = () => {
    if (!context) {
      context = new AudioContext();
      master = context.createGain();
      master.gain.value = muted ? 0 : .7;
      const limiter = context.createDynamicsCompressor();
      limiter.threshold.value = -8;
      limiter.knee.value = 8;
      limiter.ratio.value = 4;
      limiter.attack.value = .003;
      limiter.release.value = .18;
      master.connect(limiter).connect(context.destination);
      const voices = new AudioVoices(context);
      music = new SynthwaveScore(context, voices, master);
      music.setCombat(combat);
      music.start();
      const effectsBus = context.createGain();
      effectsBus.gain.value = 1.6;
      effectsBus.connect(master);
      effects = createEffects(context, voices, effectsBus, () => music.duck());
      const wind = context.createBufferSource();
      wind.buffer = voices.noiseBuffer;
      wind.loop = true;
      const filter = context.createBiquadFilter();
      filter.type = 'lowpass';
      filter.frequency.value = 150;
      const ambience = context.createGain();
      ambience.gain.value = .025;
      wind.connect(filter).connect(ambience).connect(master);
      wind.start();
    }
    if (context.state === 'suspended') context.resume().catch(() => {});
  };
  const play = (kind, gain = 1, pan = 0) => {
    if (context?.state === 'running' && !muted) effects.play(kind, gain, pan);
  };
  const setCombat = next => {
    combat = next;
    music?.setCombat(combat);
  };
  const suspend = () => context?.suspend().catch(() => {});
  document.addEventListener('pointerdown', ensure, {passive: true});
  document.addEventListener('keydown', event => {
    if (event.code !== 'KeyM' || event.repeat) return;
    muted = !muted;
    if (context) master.gain.setTargetAtTime(muted ? 0 : .7, context.currentTime, .05);
  });
  document.addEventListener('pointerlockchange', () => {
    if (!document.pointerLockElement) suspend();
  });
  document.addEventListener('visibilitychange', () => {
    if (document.hidden) suspend();
  });
  document.defaultView.addEventListener('pagehide', () => music?.stop());
  return {play, setCombat, get state() {return context?.state ?? 'locked';}};
}
