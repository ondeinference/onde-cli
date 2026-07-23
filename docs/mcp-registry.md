# MCP Registry

The Onde CLI's MCP server (`onde --mcp`, added in v0.4.0) is listed in the
[official MCP Registry](https://registry.modelcontextprotocol.io) as
**`io.github.ondeinference/onde-cli`**. The registry is a metadata index — it
doesn't host binaries — so the listing points at packages we already publish to
npm and NuGet, and clients that browse the registry can offer a one-click
install.

## What makes up the listing

| Piece | Where |
| --- | --- |
| Server metadata | [`server.json`](../server.json) at the repo root |
| npm ownership proof | `mcpName` in `npm/package-main.json.tmpl`, copied verbatim into the published manifest by `npm/scripts/render-main-package.cjs` |
| NuGet ownership proof | `<!-- mcp-name: ... -->` in `nuget/onde-cli/README.md` |
| Publishing | `.github/workflows/release-mcp-registry.yml` |

The registry checks each listed package for a marker naming the server. If the
marker is missing or doesn't match `name` in `server.json`, publishing fails
with "Registry validation failed for package".

The markers ship *inside* the published artifacts, so they have to be in place
before the npm and NuGet packages are built. A version published before the
marker landed can never be listed, since neither registry lets you overwrite a
version. The publish workflow checks the real published artifacts, not just the
sources in this repo, and stops early if a marker is missing.

**v0.4.0 cannot be listed.** It shipped the MCP server, and its NuGet package
carries the `mcp-name` marker, but `@ondeinference/cli@0.4.0` published without
`mcpName` — the template change adding it came afterwards. `server.json`
therefore targets 0.4.1, the first release where both markers are live.

### Why the name is `io.github.ondeinference/...`

The namespace has to match the authentication method. We authenticate with
GitHub OIDC from Actions, which grants the `io.github.ondeinference/*` namespace
(the reverse-DNS form of the `ondeinference` org).

The registry grants the namespace with the org's exact GitHub casing and
compares case-sensitively, so a mis-cased namespace is rejected with a 403 even
though GitHub itself treats org names case-insensitively. The `ondeinference`
org is all lowercase, which makes this a non-issue here — but every marker still
has to spell it identically.

### Which packages are listed

- **npm** — `@ondeinference/cli`, launched as `npx @ondeinference/cli --mcp`.
  The package declares a single binary (`onde`), so `npx` resolves it despite
  the name difference.
- **NuGet** — `Onde.Cli`, launched as `dnx Onde.Cli --mcp`.

PyPI is deliberately not listed. The `onde-cli` wheel installs its executable as
`onde` (maturin `bindings = "bin"`), and `uvx onde-cli` looks for an executable
matching the distribution name, so the launch command the registry hands to
clients wouldn't work. Adding PyPI would mean shipping an extra console script
named `onde-cli`. pub.dev, Homebrew, and the direct GitHub release binaries have
no registry package type at all.

## Releasing a new version

`server.json` carries the version twice, once for the server and once per
package. The publish workflow rewrites both from the tag it was given, so the
committed values are documentation rather than the source of truth — keep them
pointing at the next intended listing.

Publishing is chained off the release: pushing a `v*` tag runs the crates.io
workflow, which fans out to the distribution workflows, and **NuGet Release**
triggers **MCP Registry Release** once its publish job succeeds. It hangs off
NuGet rather than the fan-out because the listing needs both npm and NuGet live
at that version, and NuGet is the slower of the two — in the v0.4.0 release,
NuGet took 55 minutes against npm's 35.

The registry fetches the package metadata during publish, so
`@ondeinference/cli@<version>` and `Onde.Cli <version>` must already exist. npm
is queryable within seconds of publishing; nuget.org validates first and takes
roughly 5–15 minutes to reach the flat container. The workflow polls both for up
to ten minutes rather than failing on that gap.

To publish by hand — a re-run after a transient failure, say — dispatch it with
the tag:

```sh
gh workflow run release-mcp-registry.yml --ref main -f tag=v0.4.1
```

The workflow publishes the `server.json` on the branch you dispatch from, not
the one at the tag. It uploads metadata rather than building from source, and
that metadata can change after a tag is cut; the tag input only selects which
version to publish.

Confirm the listing afterwards:

```sh
curl "https://registry.modelcontextprotocol.io/v0.1/servers?search=io.github.ondeinference/onde-cli"
```

## Publishing without Actions

Rarely needed, but if Actions is unavailable:

```sh
brew install mcp-publisher
mcp-publisher login github        # device flow, needs push access to ondeinference
mcp-publisher publish             # reads ./server.json
```

Versions are immutable — republishing the same version is rejected. Fixing a bad
listing means shipping a new patch version.
