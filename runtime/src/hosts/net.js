import { makeHttpHost } from "./http.js";
export function makeNetHost() {
  return {
    ...makeHttpHost(),
    netUrlParse: (input, base) => {
      try {
        const url = base === "" ? new URL(input) : new URL(input, base);
        // URL.search/hash hide empty delimiters; inspect canonical serialization before removing them.
        const href = url.href;
        const fragmentAt = href.indexOf("#");
        const beforeFragment = fragmentAt < 0 ? href : href.slice(0, fragmentAt);
        const queryAt = beforeFragment.indexOf("?");
        const fragment = fragmentAt < 0 ? "" : href.slice(fragmentAt);
        const query = queryAt < 0 ? "" : beforeFragment.slice(queryAt);
        const pairs = Array.from(url.searchParams).flat();
        const scheme = url.protocol.slice(0, -1);
        const hostname = url.hostname;
        const path = url.pathname;
        const prefix = queryAt < 0 ? beforeFragment : beforeFragment.slice(0, queryAt);
        return [prefix, scheme, hostname, path, fragment, query, ...pairs];
      } catch (_) {
        return [];
      }
    },
  };
}
