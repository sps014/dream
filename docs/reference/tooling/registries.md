# Package registries

A registry stores published packages. Use this guide when you need a private registry or want to understand package downloads.

## Registry protocol

A registry is a sparse index plus tarball storage. A plain directory works, served as a local
`file://` path or over any static HTTP file server:

```text
<base>/index/<name>                         newline-delimited JSON, one entry per published version
<base>/dl/<name>/<name>-<version>.tar.gz    the tarball an index entry's "tarball" field points at
```

`<name>` is the full registry package name (including `.` when present), e.g. `index/foo.bar` and
`dl/foo.bar/foo.bar-1.0.0.tar.gz`. Dots are literal filename characters, not path separators.

Each line of `<base>/index/<name>` is a JSON object:

```json
{
  "name": "json-tools",
  "vers": "0.3.1",
  "deps": [{"name": "buffer-utils", "req": "^1.0"}],
  "cksum": "sha256:...",
  "tarball": "dl/json-tools/json-tools-0.3.1.tar.gz",
  "description": "JSON helpers",
  "authors": ["Jane Doe <jane@example.com>"],
  "license": "MIT",
  "edition": "2026",
  "type": "lib",
  "targets": ["native", "web"],
  "readme": "README.md",
  "keywords": ["json", "parse"]
}
```

Fields beyond `name` / `vers` / `deps` / `cksum` / `tarball` are optional metadata copied from
`dream.toml` at publish time. `readme` is an **archive-relative path** (e.g. `README.md`) pointing at
the README packed into the tarball — the index never embeds README body text.
`keywords` come from `[package].keywords` and feed static search.
`catalog.json` stores the same discovery fields except `deps` / `cksum` / `tarball`.

Optional endpoints / files for `dreamer search` / `dreamer publish`:

- `GET  <base>/search?q=<query>` → JSON array of index-entry objects (dynamic registries)
- `GET  <base>/catalog.json` → compact search catalog used when `/search` is absent (static/GitHub registries)
- `POST <base>/api/v1/publish` → JSON body `{ "entry": <index-entry>, "tarball_base64": "..." }` (non-GitHub HTTP registries)

The default public registry is the GitHub repo
[`sps014/dream-registry`](https://github.com/sps014/dream-registry), served at
`https://raw.githubusercontent.com/sps014/dream-registry/main`. Indexes live under `index/`,
tarballs under `dl/` (separate trees). Max package tarball size is **10 MiB**.

`dreamer publish` to that registry uses the GitHub Contents API (set `DREAM_REGISTRY_TOKEN` or
`--token` with `contents:write`). Point `[registries] default` at any other `file://` or
`http(s)://` location implementing the protocol above for private/offline use.

### Finding packages

- **CLI:** `dreamer search <query>` — matches package name, description, and keywords.
- **Web:** [sps014.github.io/dream-registry](https://sps014.github.io/dream-registry/) — browse and copy install commands.

The first published library is [`semver`](https://github.com/sps014/dream-packages/tree/main/semver)
(`dreamer add semver`). Official libraries live in [`sps014/dream-packages`](https://github.com/sps014/dream-packages).


## Trying it end to end without a hosted registry

Since a plain directory is a fully compliant registry, you can try the whole flow locally:

```bash
mkdir -p /tmp/my-registry
# ... publish a package into it, e.g. by running `dreamer publish` from that package's own
# project with `--registry file:///tmp/my-registry` ...

# then, in a consuming project's dream.toml:
# [registries]
# default = "file:///tmp/my-registry"
dreamer add that-package
```
