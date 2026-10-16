---
title: Registries
summary: Run a private registry and understand package downloads.
icon: material/warehouse
---

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

| Field | Required | Meaning |
|---|---|---|
| `name` | yes | Full registry package name |
| `vers` | yes | Published version |
| `deps` | yes | Dependencies as `{ "name", "req" }` objects (`[dependencies]` entries with a version) |
| `cksum` | yes | `sha256:<hex>` checksum of the tarball |
| `tarball` | yes | Tarball location, relative to the registry base unless it is an `http(s)://` URL |
| `description` | no | From `[package].description` |
| `authors` | no | From `[package].authors` |
| `license` | no | From `[package].license` |
| `edition` | no | From `[package].edition` |
| `type` | no | `bin` or `lib`, from `[package].type` |
| `targets` | no | From `[package].targets` |
| `readme` | no | README path inside the tarball |
| `keywords` | no | From `[package].keywords` |

Fields beyond `name` / `vers` / `deps` / `cksum` / `tarball` are optional metadata copied from
`dream.toml` at publish time. `readme` is an **archive-relative path** (e.g. `README.md`) pointing at
the README packed into the tarball — the index never embeds README body text.
`keywords` come from `[package].keywords` and feed static search.
`catalog.json` stores the same discovery fields except `deps` / `cksum` / `tarball`.

Optional endpoints / files for `dreamer search` / `dreamer publish`:

| Request | Returns or accepts | Used by |
|---|---|---|
| `GET <base>/search?q=<query>` | JSON array of index entries | Dynamic registries |
| `GET <base>/catalog.json` | Compact search catalog, used when `/search` is absent | Static and GitHub registries |
| `POST <base>/api/v1/publish` | JSON body `{ "entry": <index-entry>, "tarball_base64": "..." }` | HTTP registries not hosted on GitHub |

### Registry URLs

| URL form | Treated as |
|---|---|
| `https://raw.githubusercontent.com/<owner>/<repo>/<branch>` | GitHub registry; publishes through the GitHub Contents API |
| `https://<owner>.github.io/<repo>` | GitHub registry on branch `main` |
| Any other `http://` or `https://` URL | HTTP registry using the endpoints above |
| `file://<path>` or a plain path | Local directory registry |

The default public registry is the GitHub repo
[`sps014/dream-registry`](https://github.com/sps014/dream-registry), served at
`https://raw.githubusercontent.com/sps014/dream-registry/main`. Indexes live under `index/`,
tarballs under `dl/` (separate trees). Max package tarball size is **10 MiB**.

`dreamer publish` to that registry uses the GitHub Contents API (pass `--token`, or set
`DREAM_REGISTRY_TOKEN` or `GITHUB_TOKEN`, with `contents:write`). Point `[registries] default` at any other `file://` or
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
