import { makeHttpHost } from "./http.js";
import { makeSocketHost } from "./sockets.js";
import { makeWebSocketHost } from "./websocket.js";
export function makeNetHost() {
  return {
    ...makeHttpHost(),
    ...makeSocketHost(),
    ...makeWebSocketHost(),
    // The URL Standard's host parser applies UTS #46 and rejects invalid domains.
    netDomainToAscii: (domain) => {
      try {
        return new URL("https://" + domain + "/").hostname;
      } catch (_) {
        return "";
      }
    },
  };
}
