import { makeHttpHost } from "./http.js";
import { makeSocketHost } from "./sockets.js";
export function makeNetHost() {
  return {
    ...makeHttpHost(),
    ...makeSocketHost(),
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
