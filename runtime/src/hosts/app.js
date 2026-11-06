// Process lifetime for `system.app`. Event kinds and fact indexes match
// crates/dream-host-app/src/events.rs and facts.rs. Browsers report page freeze/resume,
// tab visibility, color scheme and language; Node reports SIGINT and SIGTERM only.
import { isNode } from "../platform.js";

const INTERRUPT = 1;
const TERMINATE = 2;
const SLEEPING = 4;
const WOKE = 5;
const ACTIVE = 8;
const THEME = 10;
const LOCALE = 11;

const FACT_ACTIVE = 2;
const FACT_THEME = 4;

function currentTheme() {
  const media = globalThis.matchMedia;
  if (typeof media !== "function") return -1;
  if (media("(forced-colors: active)").matches) return 2;
  return media("(prefers-color-scheme: dark)").matches ? 1 : 0;
}

function currentActive() {
  const doc = globalThis.document;
  return doc ? Number(doc.visibilityState !== "hidden") : -1;
}

export function makeAppHost() {
  let running = false;
  let queue = [];
  let waiter = null;
  let value = 0;
  let stopSignals = 0;
  let frozenAt = -1;
  let keepAlive = null;
  let detach = [];
  const facts = [-1, -1, -1, -1, -1];

  const deliver = (kind, payload) => {
    if (!running) return;
    if (waiter) {
      const resolve = waiter;
      waiter = null;
      value = payload;
      resolve(kind);
      return;
    }
    if (kind === LOCALE && queue.some(([k]) => k === kind)) return;
    if (kind === THEME && queue.some(([k, v]) => k === kind && v === payload)) return;
    queue.push([kind, payload]);
  };
  const report = (fact, kind, next) => {
    if (next < 0 || facts[fact] === next) return;
    facts[fact] = next;
    deliver(kind, next);
  };
  const listen = (target, name, handler) => {
    if (!target?.addEventListener) return;
    target.addEventListener(name, handler);
    detach.push(() => target.removeEventListener(name, handler));
  };

  const attach = () => {
    const doc = globalThis.document;
    listen(doc, "visibilitychange", () => report(FACT_ACTIVE, ACTIVE, currentActive()));
    listen(doc, "freeze", () => {
      frozenAt = performance.now();
      deliver(SLEEPING, 0);
    });
    listen(doc, "resume", () => {
      const slept = frozenAt < 0 ? -1 : Math.round(performance.now() - frozenAt);
      frozenAt = -1;
      deliver(WOKE, slept);
    });
    listen(globalThis.window, "languagechange", () => deliver(LOCALE, 0));
    if (typeof globalThis.matchMedia === "function") {
      for (const query of ["(prefers-color-scheme: dark)", "(forced-colors: active)"]) {
        listen(globalThis.matchMedia(query), "change", () => report(FACT_THEME, THEME, currentTheme()));
      }
    }
    if (isNode) {
      const signal = (kind, code) => () => {
        stopSignals += 1;
        if (stopSignals > 1) process.exit(code);
        deliver(kind, 0);
      };
      const onInterrupt = signal(INTERRUPT, 130);
      const onTerminate = signal(TERMINATE, 143);
      process.on("SIGINT", onInterrupt);
      process.on("SIGTERM", onTerminate);
      detach.push(() => {
        process.off("SIGINT", onInterrupt);
        process.off("SIGTERM", onTerminate);
      });
    }
  };

  return {
    appStart: () => {
      if (running) return 0;
      running = true;
      queue = [];
      stopSignals = 0;
      facts[FACT_ACTIVE] = currentActive();
      facts[FACT_THEME] = currentTheme();
      attach();
      return 1;
    },
    appNextEvent: () => {
      if (!running) return Promise.resolve(0);
      if (queue.length > 0) {
        const [kind, payload] = queue.shift();
        value = payload;
        return Promise.resolve(kind);
      }
      // Node exits when nothing holds the event loop open, so a pending wait keeps a timer.
      if (isNode && keepAlive === null) keepAlive = setInterval(() => {}, 1 << 30);
      return new Promise(resolve => {
        waiter = kind => {
          if (keepAlive !== null) {
            clearInterval(keepAlive);
            keepAlive = null;
          }
          resolve(kind);
        };
      });
    },
    appEventValue: () => value,
    appReply: () => {},
    appQuery: fact => facts[fact] ?? -1,
    appReport: message => console.error(message),
    appStop: () => {
      running = false;
      queue = [];
      for (const undo of detach) undo();
      detach = [];
      if (waiter) {
        const resolve = waiter;
        waiter = null;
        resolve(0);
      }
    },
  };
}
