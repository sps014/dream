import { makeHttpHost } from "./http.js";
import { makeSocketHost } from "./sockets.js";
export function makeNetHost() {
  return {
    ...makeHttpHost(),
    ...makeSocketHost(),
    netDomainToAscii: (domain) => {
      if (/[\x00-\x20\x7f%#/:<>?@[\\\]^|]/.test(domain)) return "";
      try {
        // The non-ASCII suffix forces UTS #46 processing even for ASCII input and
        // keeps URL parsing from interpreting the last input label as an IPv4 number.
        const suffix = ".xn--9ca"; // .é
        const host = new URL("https://" + domain + ".é/").hostname;
        return host.endsWith(suffix) ? host.slice(0, -suffix.length) : "";
      } catch (_) {
        return "";
      }
    },
  };
}
