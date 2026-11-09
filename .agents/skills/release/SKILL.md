---
name: release
description: Use whenever cutting, preparing, tagging, or publishing an onde-cli release — version bumps, release branch, merges, tagging, pushing to trigger the publish pipeline, and verifying it landed. Also use when a release went wrong (wrong tag target, failed crates.io verify, development left behind main, unsigned release commits) and the repo state needs repair. Trigger even if the user only says "release vX.Y.Z", "bump the version", "tag a release", or "publish the new version".
user-invocable: true
---

# Release skill

End-to-end release procedure for `onde-cli`. Complements the git skill (merge rules)
and the distribution skill (channel details) — this skill is the concrete runbook.

## Decide the version

- Look at `git tag -l 'v*' --sort=-v:refname | head -1` for the latest release.
- Look at the commits since (`git log v<latest>..development --oneline`).
  Fixes and chores only → patch bump. New user-facing features → minor bump.
- Check for an abandoned `release/vX.Y.Z` branch — its version is usually the
  intended next one. If it exists, inspect it (`git log development..release/...`)
  before deciding to reuse or delete it; do not blindly `branch -D` without looking,
  and never reuse a branch whose commits were made unsigned or by the wrong identity.

## Version bump surfaces

These files pin the crate version and must all move together:

- `Cargo.toml` — `version = "X.Y.Z"` in `[package]`
- `packaging/pub/onde_cli/pubspec.yaml` — `version: X.Y.Z`
- `packaging/pub/onde_cli/CHANGELOG.md` — a `## X.Y.Z` entry at the top. pub.dev's dry-run
  treats a changelog that doesn't mention the version as a warning and exits non-zero, which
  stopped the pub.dev releases for v0.4.2 to v0.5.0.
- `packaging/mcp/server.json` — three occurrences: top-level `version`, and the npm and NuGet
  `packages[].version` entries (search for the old version string to catch all)

`docs/mcp-registry.md` and `.github/workflows/release-mcp-registry.yml` mention
past versions only as historical examples — do not bump those.

The npm wrapper manifests are NOT bump targets: the committed `package.json` files
are rewritten at release time by `packaging/npm/scripts/render-*.cjs` from the tag.

After editing, run `cargo check` so `Cargo.lock` picks up the new version, then
`cargo check --locked` to confirm, and commit the lockfile in the bump commit.
(`--locked` refuses to rewrite the lockfile, so it can't be the step that updates it.)

## Sequence

Follow the git skill's branch flow — `release/vX.Y.Z → development → main`,
`--no-ff` at every hop, tag on the final `main` merge commit. Concretely:

```bash
git fetch <remote>                     # pre-flight: local development/main must match the remote tips
git checkout development
git pull <remote> development          # start from the pushed tip
git checkout -b release/vX.Y.Z
# ... edit the bump surfaces ...
git commit -am "Prepare vX.Y.Z"        # signed, correct identity (see below)
git checkout development
git merge --no-ff release/vX.Y.Z -m "Merge release prep for vX.Y.Z"
git checkout main
git pull <remote> main
git merge --no-ff development -m "Merge development for vX.Y.Z"
git tag vX.Y.Z
git push <remote> development main vX.Y.Z
git branch -d release/vX.Y.Z           # only after the merge lands
```

Before the checkout, compare local `development` and `main` with the remote tips
(`git rev-list --left-right --count development...<remote>/development`). If a checkout
is blocked by untracked leftovers, move them aside; don't delete them.

The remote here is the one whose URL is the sigit.si or GitHub org repo —
check `git remote -v`; this repo is commonly cloned with per-account remotes
(`gh-setoelkahfi`, `gh-paydii`, `gh-keypair34`) rather than `origin`.

### What a good result looks like

- The tag sits on the step-3 merge commit on `main` (`git log --graph main`
  shows a merge commit directly above the previous release).
- `git log development..main` and `git log main..development` are both empty
  after the merges — the two branches carry identical trees.
- Never tag a release-branch tip or a bare bump commit on `main`; if the work
  somehow already landed on `main` linearly, follow the git skill's fallback
  (normal commit, tag it) rather than rewriting history.

### Post-push back-merge check

If anything reached `main` without passing through `development` first (hotfix,
manual push, a fast-forwarded release), bring it back immediately:

```bash
git checkout development
git merge --no-ff main -m "Merge main for vX.Y.Z"
git push <remote> development
```

A release is not done until the two branches contain the same tree.

## Commits and signatures — read the git-commit-signing skill first

Release commits must be GPG-signed by the maintainer identity that GitHub can
verify (this repo: `Seto Elkahfi <1797197+setoelkahfi@users.noreply.github.com>`,
EDDSA key `2B76DF0F04014E36`). The `gh` CLI's active account is independent of
git config — set both, and verify before pushing:

```bash
git log -1 --format='%an <%ae> sig:%G? %GK'    # want sig:G and the right key
gh api repos/<owner>/<repo>/commits/<sha> --jq .commit.verification.verified
```

Every `git commit` and `git merge` in this flow creates a commit — configure
signing so merges are signed too, or amend unsigned ones before pushing
(`git commit --amend --no-edit -S`, only on unpushed commits).

Commit trailer: end release commits with the co-author trailer of the agent making
the commit, exactly as that agent's system prompt gives it, for example
`OndeCode v<version>-<surface> <noreply@ondeinference.com>` or
`siGit Code v<version>-<surface> <noreply@sigit.si>`. Don't copy a version from an
older commit or from this file.

## The tag push is the release

Pushing `vX.Y.Z` triggers a three-level chain. There is no separate "publish" step.

1. `release-crates.yml` (CLI Release Crates.io) checks the tag against `Cargo.toml` and
   `packaging/pub/onde_cli/pubspec.yaml`, publishes the crate, then dispatches
   **GitHub, npm, NuGet and PyPI**.
2. `release-github.yml` (CLI Release GitHub) then dispatches **Homebrew, pub.dev, Scoop and winget**
   once the release assets exist. A failed Scoop or winget dispatch only logs a warning
   (`core.warning`), so it is easy to miss.
3. `release-nuget.yml` (CLI Release NuGet) then dispatches the **MCP Registry Release**, which
   waits for the npm and NuGet packages at this version. A failed npm or NuGet publish
   therefore also fails the MCP Registry run.

pub.dev depends on a setting. Its automated publishing only publishes from the events enabled in
the package's admin page. `release-pub.yml` is dispatched, so "Enable publishing from
`workflow_dispatch` events" has to be on there. pub.dev's own docs only describe publishing from a
tag push, so how it treats a dispatched run is confirmed only by a real release. If the publish
step is refused, publish that version by hand (`dart pub publish` in `packaging/pub/onde_cli`).

## Publishing credentials

None of the registries use a stored token for the release workflows. A token that expires
shows up at release time as a failed publish, so don't add one back.

| Channel | Auth | What has to exist |
|---|---|---|
| npm | Trusted publishing | A trusted publisher on npmjs.com for each of the 7 packages (`@ondeinference/cli` and `-darwin-arm64`, `-darwin-x64`, `-linux-arm64`, `-linux-x64`, `-windows-arm64`, `-windows-x64`), naming `ondeinference/onde-cli` and `release-npm.yml`. Needs npm 11.5.1 or later and Node 22.14 or later, which the workflow sets up. |
| NuGet | Trusted publishing | A policy on nuget.org naming `release-nuget.yml`, and the `NUGET_USER` repo secret (the profile name, not the email). |
| pub.dev | Automated publishing | Publishing from GitHub Actions enabled in the package's admin page for this repo, tag pattern `v{{version}}`, `workflow_dispatch` events on. Leave "Require GitHub Actions environment" off, since the workflow uses none. |
| crates.io, PyPI | See `.github/workflows/release-crates.yml` and `release-pypi.yml` | |

The policy or trusted publisher names the workflow file that contains the publish job, not
the orchestrator that dispatches it. A new package needs its entry before its first publish.
npm shows a new entry as "Pending validation" until one publish succeeds, with a deadline
printed on the page; a publish through the workflow validates it.

Don't change a workflow file name without updating those entries.

Watch it land:

```bash
gh run list --branch vX.Y.Z
gh run watch <run-id> --exit-status
```

`Tests` (`ci.yml`) also runs on the tag push; both should be green. If `release-crates` fails,
no wrapper publishes — fix the cause, move the tag only if it has not published
anything yet (crates.io versions are immutable; a failed verify is safe to retry,
a published crate is not).

## Verify

After the pipeline finishes, spot-check the fan-out (all idempotent, safe to re-check):

- crates.io: `cargo search onde-cli --limit 1`
- GitHub Release: `gh release view vX.Y.Z`
- npm: `npm view @ondeinference/cli version`
- PyPI: `https://pypi.org/pypi/onde-cli/json` → `info.version`
- NuGet: `https://api.nuget.org/v3-flatcontainer/onde.cli/index.json`. `dotnet nuget push` printing
  "Your package was pushed" means nuget.org accepted it, not that it is listed. The package is
  large, so validation can take a while. If the version is still missing well past 30 minutes,
  check the package's Manage page on nuget.org and your email for a failed validation.
- pub.dev: `https://pub.dev/api/packages/onde_cli` → `latest.version` (new versions can take
  up to 10 minutes to appear after the upload is accepted)
- MCP Registry: the `MCP Registry Release` run is green
- Scoop / winget: `gh run list --workflow release-scoop.yml` / `--workflow release-winget.yml`
  show a run for the tag (they can silently not dispatch). winget lands as a PR to
  `microsoft/winget-pkgs` and only appears after Microsoft's review.
- Homebrew/pub.dev trail the GitHub Release; they may take a few extra minutes

## Failure modes seen before

- Fast-forwarding `main` to the bump commit and tagging it — leaves no merge
  commit, and leaves `development` behind. Back-merge `main` into `development`
  right away and prefer merge commits next time.
- Deleting a leftover `release/` branch that contained unsigned bump commits and
  redoing the bump fresh — fine, but check it first.
- Forgetting `Cargo.lock` in the bump commit.
- Stale local `development`/`main` (once 12 commits behind the remote), with checkout
  blocked by untracked leftovers from a branch made before the `packaging/` move, at
  paths the newer branch tracks. Fetch first, move the leftovers aside, don't delete.
- Pushing with the wrong `gh` account active — the push works, but PR/issue
  actions afterwards get attributed to the wrong identity.
- A stored registry token expiring. NuGet (`403 The specified API key has expired`) and npm
  (`E404 PUT` on every platform package, because npm answers a bad credential with a 404) both
  failed this way. Both now use trusted publishing. If either error comes back, check the
  trusted publisher entries before looking for a token.
- pub.dev dry-run failing on `CHANGELOG.md doesn't mention current version`. The changelog
  entry is a bump surface; add it before tagging.
- The release-github.yml dispatch passes the tag as the ref, so a workflow run for a tag uses the
  workflow file from that tagged commit. Fixing a workflow doesn't help a tag that already
  exists. Start a fresh run from the fixed branch instead:
  `gh workflow run <file> --ref <branch> -f tag=<tag>`. pub.dev is the exception, because a run
  from a branch ref may not match the tag pattern, so publish that version by hand
  (`dart pub publish` in `packaging/pub/onde_cli`) or cut the next tag.
- pub.dev publishing was moved to tag pushes for a while (`workflow_dispatch` was off in its
  settings), which took it out of the fan-out. It is dispatched again, which needs
  `workflow_dispatch` on in the pub.dev settings.
- A failed npm release leaves the MCP Registry run failing with "still not on npm after 10
  minutes". Fix npm, publish it, then re-dispatch the MCP Registry release for the tag.
- Re-running pub.dev for an old tag. It checks out that tag, so it uses that tag's pubspec and
  changelog and fails if they predate the changelog rule.
