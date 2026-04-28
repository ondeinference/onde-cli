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
- If release prep happens on a branch, merge it into `main` with `--no-ff` before tagging.
- Tag the merge commit that contains the final release state.

## Release guidance

For releases, prefer a flow like this:

```bash
git checkout main
git merge --no-ff <release-branch> -m "Merge release prep for vX.Y.Z"
git tag vX.Y.Z
git push origin main
git push origin vX.Y.Z
```

If the work is already on `main`, make a normal commit and tag that commit. Do not rewrite history just to avoid a merge commit.

## Avoid

- `git merge --ff`
- `git pull --ff-only` as a default recommendation for this repo's merge policy
- Tagging a branch tip that has not been merged into `main` when the intended release source of truth is `main`

## Why

The repo wants explicit history. Release commits should be easy to find, easy to audit, and clearly connected to the branch that introduced them.
