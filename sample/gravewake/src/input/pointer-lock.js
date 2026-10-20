export function attachPointerLock(canvas, document, report) {
  let captured = false;
  let pending = false;
  const pause = () => {
    canvas.dispatchEvent(new KeyboardEvent('keydown', { code: 'Escape', key: 'Escape' }));
    canvas.dispatchEvent(new KeyboardEvent('keyup', { code: 'Escape', key: 'Escape' }));
  };
  const request = async event => {
    if (event.button !== 0 || document.pointerLockElement === canvas || pending) return;
    if (!canvas.requestPointerLock) {
      report('Mouse capture is unavailable here. Open this page in desktop Chrome or Edge; arrow keys can also turn.');
      return;
    }
    pending = true;
    try {
      // Request during the click gesture, before the asynchronous Dream frame loop.
      await canvas.requestPointerLock();
    } catch {
      report('Click again to capture the mouse. If capture stays blocked, open the game in desktop Chrome or Edge.');
    } finally {
      pending = false;
    }
  };
  const changed = () => {
    const locked = document.pointerLockElement === canvas;
    if (locked) report('');
    else if (captured) pause();
    captured = locked;
  };
  canvas.addEventListener('pointerdown', request);
  document.addEventListener('pointerlockchange', changed);
  return () => {
    canvas.removeEventListener('pointerdown', request);
    document.removeEventListener('pointerlockchange', changed);
  };
}
