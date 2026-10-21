# What Tagr is 
Tagr is a non-destructive metadata overlay that unifies static user-assigned labels (tags, notes) with dynamic environmental predicates (file size, git status) into a queryable index over registered files.

# What Tagr is NOT 
It is not a file manager, disk crawler, or full-text indexer, delegating all file mutations, moves, and deletions entirely to standard Unix tools.

# Who it serves 
It serves terminal power users who compose CLI pipelines, treating TUIs, pickers, and editor interfaces strictly as secondary presentation shells over atomic CLI primitives.

# What it trusts the filesystem to do
It trusts the filesystem as the sole authority for file bytes, paths, and content. Tagr identifies files across renames and moves without committing to a platform-specific identity scheme.

# How it treats user data 
It treats user-authored metadata as an independent, portable layer that never alters target files and guarantees zero vendor lock-in, leaving the underlying database architecture to be decided in a dedicated storage ADR.

# North Star
Tagr indexes explicit labels and dynamic predicates to yield path streams for Unix.
