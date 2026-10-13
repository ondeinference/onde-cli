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
- `pub/onde_cli/pubspec.yaml` — `version: X.Y.Z`
- `server.json` — three occurrences: top-level `version`, and the npm and NuGet
  `packages[].version` entries (search for the old version string to catch all)

`docs/mcp-registry.md` and `.github/workflows/release-mcp-registry.yml` mention
past versions only as historical examples — do not bump those.

The npm wrapper manifests are NOT bump targets: the committed `package.json` files
are rewritten at release time by `npm/scripts/render-*.cjs` from the tag.

After editing, run `cargo check --locked` (or let the build update `Cargo.lock`)
so the lockfile carries the new version, and commit it in the same bump commit.

## Sequence

Follow the git skill's branch flow — `release/vX.Y.Z → development → main`,
`--no-ff` at every hop, tag on the final `main` merge commit. Concretely:

```bash
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

Commit trailer: end release-related commit messages with

```
Co-Authored-By: siGit Code v1.6.7-acp <noreply@sigit.si>
```

## The tag push is the release

Pushing `vX.Y.Z` triggers `release-crates.yml`, which verifies the tag against
`Cargo.toml` and `pubspec.yaml`, publishes the crate, then dispatches the
wrapper workflows (GitHub Releases, npm, NuGet, PyPI, then Homebrew and pub.dev
after the GitHub Release assets exist). There is no separate "publish" step.

Watch it land:

```bash
gh run list --branch vX.Y.Z
gh run watch <run-id> --exit-status
```

`CI` also runs on the tag push; both should be green. If `release-crates` fails,
no wrapper publishes — fix the cause, move the tag only if it has not published
anything yet (crates.io versions are immutable; a failed verify is safe to retry,
a published crate is not).

## Verify

After the pipeline finishes, spot-check the fan-out (all idempotent, safe to re-check):

- crates.io: `cargo search onde-cli --limit 1`
- GitHub Release: `gh release view vX.Y.Z`
- npm: `npm view onde-cli version`
- PyPI: `https://pypi.org/pypi/onde-cli/json` → `info.version`
- NuGet: flat container index for `onde-cli`
- Homebrew/pub.dev trail the GitHub Release; they may take a few extra minutes

## Failure modes seen before

- Fast-forwarding `main` to the bump commit and tagging it — leaves no merge
  commit, and leaves `development` behind. Back-merge `main` into `development`
  right away and prefer merge commits next time.
- Deleting a leftover `release/` branch that contained unsigned bump commits and
  redoing the bump fresh — fine, but check it first.
- Forgetting `Cargo.lock` in the bump commit.
- Pushing with the wrong `gh` account active — the push works, but PR/issue
  actions afterwards get attributed to the wrong identity.
