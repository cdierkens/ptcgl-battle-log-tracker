---
name: implement
description: "Implement a piece of work based on a spec or set of tickets."
disable-model-invocation: true
---

Implement the work described by the user in the spec or tickets.

## Isolation and cleanup

Work in a dedicated worktree branched from the latest base branch. Do not
implement in the primary checkout. After the work is merged:

```bash
PRIMARY=$(git worktree list --porcelain | awk '/^worktree /{print $2; exit}')
git -C "$PRIMARY" fetch origin main --prune
git -C "$PRIMARY" switch main
git -C "$PRIMARY" merge --ff-only origin/main
git worktree remove <worktree>
git branch -d <branch>
git -C "$PRIMARY" status -sb   # must not show [behind N]
```

If the primary is dirty, move the uncommitted work to a named worktree/branch
first; never overwrite it. Never finish with the primary behind or on a feature
branch.

If the user passes a ticket reference, fetch it from the issue tracker and state its title before starting. If the reference is ambiguous, ask.

Call the Skill tool with "tdd" where possible, at pre-agreed seams.

Run typechecking regularly, single test files regularly, and the full test suite once at the end.

Once done, call the Skill tool with "code-review" to review the work.

Commit your work to the current branch.
