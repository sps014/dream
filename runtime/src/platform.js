/** True when running under Node (vs. a browser). */
export const isNode = typeof process !== "undefined" && !!(process.versions && process.versions.node);

let _nodeFs = null;
let _nodeCrypto = null;
let _nodeChildProcess = null;
let _nodeNet = null;

export function setNodeFs(fs) { _nodeFs = fs; }
export function setNodeCrypto(crypto) { _nodeCrypto = crypto; }
export function setNodeChildProcess(childProcess) { _nodeChildProcess = childProcess; }
export function setNodeNet(net) { _nodeNet = net; }
export function getNodeFs() { return _nodeFs; }
export function getNodeCrypto() { return _nodeCrypto; }
export function getNodeChildProcess() { return _nodeChildProcess; }
export function getNodeNet() { return _nodeNet; }
