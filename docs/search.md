# Search

Two layers: `libwau::catalogue::search` is the pure fuzzy-matching/ranking algorithm over a
`ComputedCatalogue`; `wau search` (`wau/src/app/mod.rs`'s `cmd_search`) is the interactive
CLI flow built on top of it — fetch the catalogue, run the search, print numbered results,
prompt for a selection, install what's picked.

---

## Catalogue fuzzy search (`libwau::catalogue::search`)

Uses [`frizbee`](https://docs.rs/frizbee)'s Smith-Waterman-based fuzzy matcher: a query's
characters must appear, in order, somewhere in a candidate's normalised name, with bonuses for
prefix/exact matches. Unlike a plain edit-distance ratio, this also matches abbreviation-style
queries — `dbm` matches `Deadly Boss Mods`. "Normalised" here is `catalogue::normalise_name`:
strip everything that isn't ASCII alphanumeric, casefold what's left.

### Filter pipeline

`search()` applies filters in this order, all before any scoring happens:

1. **Flavour** — `entry.game_flavours.contains(&flavour)`, always applied, not optional. A
   search only ever sees entries compatible with the calling profile's active flavour.
2. **`options.sources`** — if non-empty, only entries whose `source` is in the list.
3. **`options.start_date`** — if set, only entries with `last_updated >= start_date`.
4. **`options.filter_installed`** (`FilterInstalled`):
   - `Ident` — no filtering by install state (the default; installed entries are still shown,
     just tagged — see the CLI section below).
   - `IncludeOnly` — only entries matching an installed `(source, id)`.
   - `Exclude` — drop entries matching an installed `(source, id)`.
   - `ExcludeFromAllSources` — like `Exclude`, but also drops every cross-source `same_as`
     equivalent of each installed entry (so installing an addon from CurseForge also hides its
     GitHub/WoWInterface/etc. listings from later searches).
5. **`options.prefer_source`** — if set, **drops** any entry whose `same_as` list references
   `prefer_source`. This is an exclusion filter, not a sort key: `--prefer-source curse` hides
   the WoWInterface/GitHub/etc. listing of an addon that also exists on CurseForge — it does not
   move CurseForge's own listing to the top. Read it as "hide the alternatives when a preferred
   source already covers this."

### Scoring

- `search_terms == "*"` is a special case: every remaining candidate gets score `0.0`, which
  (given the blend formula below) means ranking collapses entirely to download popularity — `*`
  is "show me everything, most popular first," not a fuzzy match against a literal `*`.
- Otherwise, `frizbee::Matcher` scores every candidate's `normalised_name` against the
  normalised query. frizbee's score is an unbounded, bonus-laden `u16`, not a 0.0–1.0 ratio, so
  it's normalised against the query's own best-possible match against itself (`max_score`,
  always achievable, always the ceiling for that query regardless of frizbee's internal scoring
  constants).
- The normalised match score and `entry.derived_download_score` (each source's download count
  divided by that same source's own maximum — see `catalogue::mod`) are blended 50/50
  (`EDIT_WEIGHT`/`DOWNLOAD_WEIGHT`, both `0.5`) and sorted descending.
- Results are truncated to `options.limit` **after** sorting the full filtered/scored set — the
  limit doesn't affect which candidates get scored, only how many are returned.

### Caveats

- **Search results never carry a version.** `CatalogueEntry` has no version field at all — only
  `download_count`/`last_updated`/`same_as`/etc. What `wau search` shows next to an installed
  match is the *installed* version from the lock file, not anything from the catalogue. The
  catalogue is a discovery index, not a source of version truth: whatever you actually get on
  `install` is whatever the live source resolves at that moment, which is not guaranteed to be
  what was "current" when the catalogue was last synchronised (see `docs/caching.md`).
- **Download popularity is per-source, not global.** `derived_download_score` normalises each
  entry against its *own source's* maximum download count, not across all sources. A CurseForge
  entry with score `0.9` and a Tukui entry with score `0.9` are each near the top of their own
  source's popularity distribution — they are not necessarily comparably popular in absolute
  terms. The blended ranking formula treats these as directly comparable regardless.
- **Normalisation is ASCII-only.** `normalise_name` strips everything that isn't ASCII
  alphanumeric before matching. Non-ASCII characters in either the query or a catalogue name
  (accents, non-Latin scripts) are dropped entirely rather than folded/transliterated — matching
  degrades to comparing whatever ASCII-alphanumeric residue is left.
- **Flavour-scoped, no cross-flavour search.** There's no way to search "across every flavour" in
  one call — the filter is always the calling profile's single active flavour.

---

## CLI flow (`wau search`)

`cmd_search` (`wau/src/app/mod.rs`):

1. Resolves the catalogue fresh via `catalogue::synchronise` on every invocation — not cached at
   the search-result level (only the underlying catalogue *data* is cached; see
   `docs/caching.md`). There's no persistent "last search" state between runs.
2. Runs `search::search` with options built from CLI flags: `--limit` (1–20, default 10),
   `--source` (repeatable), `--prefer-source`, `--start-date` (`YYYY-MM-DD`), and
   `--exclude-installed` (maps to `FilterInstalled::ExcludeFromAllSources` when set, `Ident`
   otherwise — installed entries are shown and tagged by default, not hidden; this is the
   opposite of the flag's pre-rename default, see `docs/WAU_RS_PLAN.md`'s revision history).
3. Prints results via `output::format_search_results`: **best match printed last**, numbered
   bottom-to-top (`1` is the best match, immediately above the input prompt) — a deliberate
   `paru`-style layout, not a bug. Each line is `N source/slug [downloads↓]`, with `[Installed:
   <version>]` appended when the entry matches something in the lock file, and the addon's
   display name indented underneath.
4. Reads a freeform selection line (`prompts::read_line`) and parses it with `parse_selection`:
   space- and/or comma-separated numbers and/or `a-b` ranges (either order, `5-2` behaves like
   `2-5`). Invalid, out-of-range, and duplicate tokens are silently dropped rather than
   rejected — there's no "invalid input, try again" loop.
5. Builds one `Defn` per selected entry (`slug` if non-empty, else `id`, as the alias) and runs
   them through the ordinary `pkg_management::install` path — the exact same resolve/download/
   progress-bar machinery as `wau install` (see `docs/caching.md` and the progress-reporting
   entry in `docs/WAU_RS_PLAN.md`'s revision history), so selections are **re-resolved against
   the live source**, not installed from whatever the catalogue snapshot said.

### Caveats

- **An empty/all-invalid selection is not an error.** `parse_selection` returning nothing prints
  "Nothing selected." and exits `0` — indistinguishable, from the exit code alone, from "search
  itself found nothing."
- **Selection re-resolves live, so a pick can fail or differ after the fact.** Because step 5
  re-resolves rather than trusting the catalogue snapshot, an entry visible in search results can
  still fail to install (removed upstream, source down, strategy mismatch) or install a version
  newer than whatever prompted the search in the first place.
- **No paging.** `--limit` caps results at 20 (clap's `value_parser` range), and there is no way
  to see "the next 20" for a broad query — narrow the query or add `--source`/`--start-date`
  instead.
