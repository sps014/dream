import { makeCliHost } from "./hosts/cli.js";
import { makeCancellationHost } from "./hosts/cancellation.js";
import { makeJsHost } from "./hosts/js.js";
import { makeNetHost } from "./hosts/net.js";
import { makeFsHost } from "./hosts/fs.js";
import { makeCryptoHost } from "./hosts/crypto.js";
import { makeConsoleProcessHost } from "./hosts/console_process.js";
import { makeDatetimeTextHost } from "./hosts/datetime_text.js";
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
    ...makeFsHost(),
    ...makeNetHost(),
    ...makeCliHost(),
    ...makeCryptoHost(),
    ...makeDatetimeTextHost(),
    ...makeConsoleProcessHost(),
    ...makeWindowHost(),
    ...makeGpuHost(),
  };
}

export {
  makeJsHost,
  makeCancellationHost,
  makeFsHost,
  makeNetHost,
  makeCliHost,
  makeCryptoHost,
  makeConsoleProcessHost,
  makeDatetimeTextHost,
  makeWindowHost,
  makeGpuHost,
};
