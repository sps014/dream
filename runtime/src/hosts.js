import { makeJsHost } from "./hosts/js.js";
import { makeHttpHost } from "./hosts/http.js";
import { makeFsHost } from "./hosts/fs.js";
import { makeCryptoHost } from "./hosts/crypto.js";
import { makeGpuHost } from "./hosts/gpu.js";
import { makeConsoleProcessHost } from "./hosts/console_process.js";
import { makeDatetimeTextHost } from "./hosts/datetime_text.js";
import { makeNetSocketsHost } from "./hosts/net_sockets.js";

/**
 * Full built-in `Dream` host module (every optional chunk). Selective runtimes compose a subset
 * of these factories instead.
 */
export function defaultDreamModule(getInstance) {
  return {
    ...makeGpuHost(getInstance),
    ...makeJsHost(getInstance),
    ...makeHttpHost(),
    ...makeFsHost(),
    ...makeCryptoHost(),
    ...makeDatetimeTextHost(),
    ...makeConsoleProcessHost(),
    ...makeNetSocketsHost(),
  };
}

export {
  makeJsHost,
  makeHttpHost,
  makeFsHost,
  makeCryptoHost,
  makeGpuHost,
  makeConsoleProcessHost,
  makeDatetimeTextHost,
  makeNetSocketsHost,
};
