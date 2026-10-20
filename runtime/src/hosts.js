import { makeCancellationHost } from "./hosts/cancellation.js";
import { makeDataHost } from "./hosts/data.js";
import { makeJsHost } from "./hosts/js.js";
import { makeNetHost } from "./hosts/net.js";
import { makeFsHost } from "./hosts/fs.js";
import { makeCryptoHost } from "./hosts/crypto.js";
import { makeConsoleProcessHost } from "./hosts/console_process.js";
import { makeDatetimeTextHost } from "./hosts/datetime_text.js";
import { makeAudioHost } from "./hosts/audio.js";
import { makeAudioMediaHost } from "./hosts/audio_media.js";
import { makeAudioCaptureHost } from "./hosts/audio_capture.js";
import { makeWindowHost } from "./hosts/window.js";
import { makeGpuHost } from "./hosts/gpu.js";

/**
 * Full built-in `Dream` host module (every optional chunk). Selective runtimes compose a subset
 * of these factories instead.
 */
export function defaultDreamModule(getInstance) {
  return {
    ...makeJsHost(getInstance),
    ...makeCancellationHost(getInstance),
    ...makeDataHost(),
    ...makeFsHost(),
    ...makeNetHost(),
    ...makeCryptoHost(),
    ...makeDatetimeTextHost(),
    ...makeConsoleProcessHost(),
    ...makeWindowHost(),
    ...makeAudioHost(),
    ...makeAudioMediaHost(),
    ...makeAudioCaptureHost(),
    ...makeGpuHost(getInstance),
  };
}

export {
  makeJsHost,
  makeCancellationHost,
  makeDataHost,
  makeFsHost,
  makeNetHost,
  makeCryptoHost,
  makeConsoleProcessHost,
  makeDatetimeTextHost,
  makeWindowHost,
  makeAudioHost,
  makeAudioMediaHost,
  makeAudioCaptureHost,
  makeGpuHost,
};
