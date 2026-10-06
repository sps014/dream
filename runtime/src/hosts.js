import { makeJsHost } from "./hosts/js.js";
import { makeFsHost } from "./hosts/fs.js";
import { makeCryptoHost } from "./hosts/crypto.js";
import { makeConsoleProcessHost } from "./hosts/console_process.js";
import { makeDatetimeTextHost } from "./hosts/datetime_text.js";

/**
 * Full built-in `Dream` host module (every optional chunk). Selective runtimes compose a subset
 * of these factories instead.
 */
export function defaultDreamModule(getInstance) {
  return {
    ...makeJsHost(getInstance),
    ...makeFsHost(),
    ...makeCryptoHost(),
    ...makeDatetimeTextHost(),
    ...makeConsoleProcessHost(),
  };
}

export {
  makeJsHost,
  makeFsHost,
  makeCryptoHost,
  makeConsoleProcessHost,
  makeDatetimeTextHost,
};
