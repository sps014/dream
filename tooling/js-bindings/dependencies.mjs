import fs from "node:fs/promises";
import path from "node:path";
import { spawnSync } from "node:child_process";

export async function dependencies(root, npm) {
  const directory = path.join(root, ".dream", "npm");
  const manifest = {
    name: "dream-js-dependencies", private: true, version: "0.0.0",
    dependencies: Object.fromEntries(Object.entries(npm).sort().map(([name, version]) => [name,
      version.startsWith("file:") ? `file:${path.relative(directory, path.resolve(root, version.slice(5)))}` : version])),
  };
  await fs.mkdir(directory, { recursive: true });
  const text = JSON.stringify(manifest, null, 2) + "\n";
  const current = await fs.readFile(path.join(directory, "package.json"), "utf8").catch(() => "");
  const lockfile = path.join(root, "dream.npm.lock");
  const lock = await fs.readFile(lockfile, "utf8").catch(() => "");
  const installed = await fs.readFile(path.join(directory, ".installed"), "utf8").catch(() => "");
  if (current === text && installed === lock && lock && await fs.stat(path.join(directory, "node_modules")).then(stat => stat.isDirectory()).catch(() => false)) return directory;
  if (!Object.keys(npm).length) return directory;
  if (process.env.DREAM_NO_AUTO_INSTALL === "1") {
    throw new Error("npm dependencies are not cached and DREAM_NO_AUTO_INSTALL=1 disables installation");
  }
  await fs.writeFile(path.join(directory, "package.json"), text);
  let locked = false;
  if (lock) {
    const parsed = JSON.parse(lock);
    locked = JSON.stringify(parsed.packages?.[""]?.dependencies ?? {}) === JSON.stringify(manifest.dependencies);
    if (locked) await fs.writeFile(path.join(directory, "package-lock.json"), lock);
  }
  const args = [
    locked ? "ci" : "install", "--ignore-scripts", "--no-audit", "--no-fund",
  ];
  // Batch files require cmd.exe; only fixed npm arguments enter its command line.
  const windows = process.platform === "win32";
  const result = spawnSync(windows ? (process.env.ComSpec || "cmd.exe") : "npm",
    windows ? ["/d", "/s", "/c", "npm.cmd", ...args] : args,
    { cwd: directory, encoding: "utf8", timeout: 180_000, windowsHide: true });
  if (result.error || result.status !== 0) {
    throw new Error(`installing [npm] dependencies: ${result.error?.message ?? result.stderr}`);
  }
  const updated = await fs.readFile(path.join(directory, "package-lock.json"), "utf8");
  await fs.writeFile(lockfile, updated);
  await fs.writeFile(path.join(directory, ".installed"), updated);
  return directory;
}
