#!/bin/sh
# Run directly by wau (its own shebang picks the interpreter — bash, sh, nu,
# whatever), so it must stay executable: chmod +x build.sh.
#
# Env vars wau sets before running this: srcdir (the git checkout),
# pkgdir (empty; populate it with the final addon folder(s)), pkgname,
# pkgver, startdir (this addbuild's own directory, for sibling files like
# patches).
#
# Details is NOT a single-folder addon: plugins/<Name>/ in the repo are
# each their own addon (own top-level .toc), and the real CurseForge
# distribution installs them as sibling AddOns folders alongside Details
# itself — confirmed against CurseForge's own API `modules` list for the
# live file (mod id 61284) and a real installed copy: Details,
# Details_Compare2, Details_DataStorage, Details_EncounterDetails,
# Details_RaidCheck, Details_Streamer, Details_TinyThreat, Details_Vanguard.
# A plain "copy everything into one folder" build (what TomTom/ZSBT use)
# would nest the plugins at Details/plugins/Details_X/, one level too deep
# for the game to ever see them as addons — wau's zip-folder detection only
# recognises exactly-one-level-deep `<Folder>/<Folder>.toc` entries, same
# rule a real CurseForge/GitHub zip goes through.
set -e

workdir=$(mktemp -d)
trap 'rm -rf "$workdir"' EXIT
git -C "$srcdir" archive HEAD | tar -x -C "$workdir"

mkdir -p "$pkgdir/$pkgname"
for entry in "$workdir"/* "$workdir"/.[!.]*; do
	[ -e "$entry" ] || continue
	name=$(basename "$entry")
	if [ "$name" = "plugins" ]; then
		for plugin in "$entry"/*; do
			[ -d "$plugin" ] || continue
			cp -r "$plugin" "$pkgdir/$(basename "$plugin")"
		done
	else
		cp -r "$entry" "$pkgdir/$pkgname/"
	fi
done
