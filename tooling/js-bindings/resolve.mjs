import path from "node:path";
import { createHash } from "node:crypto";
import { remote } from "./remote.mjs";

export const digest = text => createHash("sha256").update(text).digest("hex").slice(0, 16);

export async function resolveImport(entry, request) {
  const { specifier, file } = entry;
  if (specifier.startsWith("https://")) return remote(specifier, request.root);
  if (specifier.startsWith(".") || path.isAbsolute(specifier)) return path.resolve(path.dirname(file), specifier);
  const packageName = specifier.startsWith("@") ? specifier.split("/").slice(0, 2).join("/") : specifier.split("/")[0];
  if (!Object.hasOwn(request.npm, packageName) && !specifier.startsWith("node:")) throw new Error(`JavaScript package '${packageName}' is not declared in [npm] in dream.toml`);
  return specifier;
}
