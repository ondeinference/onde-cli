---
name: git
description: Git workflow rules for this repository.
allowed-tools: Read, Write, Edit, Glob, Grep, AskUserQuestion
user-invocable: true
---

# Git skill

## Core rule

- Use `--no-ff`. Important.

## What this means in practice

- Do not fast-forward merge feature branches, release branches, or hotfix branches.
- Prefer explicit merge commits so the branch history stays visible.
- Release prep goes through `development` on its way to `main`, with `--no-ff` at every hop.
- Tag the merge commit that contains the final release state.

## Branch flow

Feature branches merge into `development`. Releases go:

```
release/vX.Y.Z -> development -> main
```

Never merge a release branch straight into `main` — `development` has to carry the
release state too, or the next feature branch starts from a base that is missing it.

## Release guidance

```bash
# 1. cut the release branch from development, commit the version bumps
git checkout development
git checkout -b release/vX.Y.Z
git commit -am "Prepare vX.Y.Z"

# 2. release branch -> development
git checkout development
git merge --no-ff release/vX.Y.Z -m "Merge release prep for vX.Y.Z"

# 3. development -> main
git checkout main
git merge --no-ff development -m "Merge development for vX.Y.Z"

# 4. tag the main merge commit, then push
git tag vX.Y.Z
git push origin development
git push origin main
git push origin vX.Y.Z
```

The tag belongs on the step-3 merge commit on `main`, not on the release branch and not
on `development`. Pushing the tag is what triggers the wrapper publish workflows, which
read the version from the tag.

If the work is already on `main`, make a normal commit and tag that commit. Do not rewrite history just to avoid a merge commit.

## Avoid

- `git merge --ff`
- `git pull --ff-only` as a default recommendation for this repo's merge policy
- Tagging a branch tip that has not been merged into `main` when the intended release source of truth is `main`
- Merging a release branch directly into `main`, skipping `development`

## Why

The repo wants explicit history. Release commits should be easy to find, easy to audit, and clearly connected to the branch that introduced them.
