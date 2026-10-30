// Capability factories share a bridge only within one runtime instance. Numeric GPU handles
// are private to that instance and must never resolve through another instance's table.
const drawingGpuBridges = new WeakMap();
export function drawingGpuBridge(getInstance) {
  if (typeof getInstance !== "function") return null;
  let bridge = drawingGpuBridges.get(getInstance);
  if (!bridge) {
    bridge = {};
    drawingGpuBridges.set(getInstance, bridge);
  }
  return bridge;
}

