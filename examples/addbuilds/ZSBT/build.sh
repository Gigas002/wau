#!/bin/sh
# Run directly by wau (its own shebang picks the interpreter — bash, sh, nu,
# whatever), so it must stay executable: chmod +x build.sh.
#
# Env vars wau sets before running this: srcdir (the git checkout),
# pkgdir (empty; populate it with the final addon folder(s)), pkgname,
# pkgver, startdir (this addbuild's own directory, for sibling files like
# patches).
#
# ZSBT's repo root already looks like the in-game addon folder (its own
# ZSBT.toc sits at the top), so no real build step is needed — just export
# the tracked tree (`git archive`, not a plain copy: skips .git and any
# untracked/ignored cruft) into the correctly-named destination folder.
set -e
mkdir -p "$pkgdir/$pkgname"
git -C "$srcdir" archive HEAD | tar -x -C "$pkgdir/$pkgname"
