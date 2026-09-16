//! Command dispatch onto the `libwau` API assembled by [`crate::ctx`]. No
//! addon logic lives here beyond `Defn`/installed-package lookup glue;
//! every actual operation delegates to `libwau`.

use std::collections::HashSet;

use libwau::{
    catalogue::{
        self,
        search::{self, FilterInstalled, SearchOptions},
    },
    config::{ConfigError, GlobalConfig, ProfileConfig},
    lockfile::Pkg,
    matchers,
    model::Defn,
    pkg_management,
    results::{Failure, ManagerError},
    sources::{PkgCandidate, Resolver},
};

use crate::{
    cli::{
        Cli, Command, InitArgs, InstallArgs, ListFormat, ProfileCommand, RemoveArgs, ReplaceArgs,
        SearchArgs, SyncArgs,
    },
    ctx::{self, AppCtx, CtxError},
    output::{any_errors, format_results},
    prompts::{self, Choice},
    style,
};

#[cfg(test)]
mod tests;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error(transparent)]
    Ctx(#[from] CtxError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Lock(#[from] libwau::lockfile::LockError),
    #[error(transparent)]
    Http(#[from] libwau::http::HttpError),
    #[error(transparent)]
    Failure(#[from] Failure),
    #[error(transparent)]
    Prompt(#[from] inquire::InquireError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Other(String),
}

pub async fn run(cli: &Cli) -> Result<i32, AppError> {
    match &cli.command {
        Command::Install(args) => cmd_install(cli, args).await,
        Command::Sync(args) => cmd_sync(cli, args).await,
        Command::Remove(args) => cmd_remove(cli, args).await,
        Command::Replace(args) => cmd_replace(cli, args).await,
        Command::Init(args) => cmd_init(cli, args).await,
        Command::Search(args) => cmd_search(cli, args).await,
        Command::List(args) => cmd_list(cli, &args.addons, args.format).await,
        Command::Info(args) => {
            cmd_list(cli, std::slice::from_ref(&args.addon), ListFormat::Detailed).await
        }
        Command::Profile(ProfileCommand::Erase) => {
            cmd_profile_erase(cli)?;
            Ok(0)
        }
    }
}

// ============================================================================
// Ctx bootstrap
// ============================================================================

/// Builds the [`AppCtx`] for `cli.profile`. Every command uses this — there is
/// no interactive bootstrap: a profile that isn't configured yet must be
/// hand-written (see `examples/config.toml` and
/// `examples/profiles/example/profile.toml`), then picked up by `init` or any
/// other command.
fn load_ctx(cli: &Cli) -> Result<AppCtx, AppError> {
    AppCtx::build(cli).map_err(|e| match e {
        CtxError::Config(e) => describe_config_error(cli, e),
        e => e.into(),
    })
}

/// Turns a [`ConfigError`] into a message pointing at the specific `-p`/
/// hand-write fix, rather than the terse library-level Display each variant
/// otherwise has.
fn describe_config_error(cli: &Cli, e: ConfigError) -> AppError {
    match e {
        ConfigError::NotFound { path } => AppError::Other(format!(
            "profile '{}' isn't configured (expected {}); hand-write it — see \
             examples/config.toml and examples/profiles/example/profile.toml",
            cli.profile.as_deref().unwrap_or("?"),
            path.display()
        )),
        ConfigError::NoProfilesConfigured => AppError::Other(
            "no profiles configured; hand-write one — see examples/config.toml and \
             examples/profiles/example/profile.toml"
                .to_owned(),
        ),
        ConfigError::AmbiguousProfile { available } => AppError::Other(format!(
            "multiple profiles configured ({}); specify one with -p/--profile",
            available.join(", ")
        )),
        e => e.into(),
    }
}

// ============================================================================
// install / sync / remove
// ============================================================================

async fn cmd_install(cli: &Cli, args: &InstallArgs) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
    let AppCtx {
        profile,
        mut lock,
        http,
        sources,
        download_locks,
    } = app_ctx;
    let defns = parse_defns(&args.addons, &sources)?;
    let pkg_ctx = pkg_management::Ctx {
        http: &http,
        sources: &sources,
        download_locks: &download_locks,
        addon_dir: &profile.addon_dir,
        cache_dir: &profile.global_config.dirs.cache,
        flavour: profile.product.flavour(),
    };

    let results =
        pkg_management::install(&mut lock, &pkg_ctx, &defns, args.replace, args.dry_run).await;
    println!("{}", format_results(&results, style::color_enabled()));
    Ok(i32::from(any_errors(&results)))
}

async fn cmd_sync(cli: &Cli, args: &SyncArgs) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
    let AppCtx {
        profile,
        mut lock,
        http,
        sources,
        download_locks,
    } = app_ctx;
    let pkg_ctx = pkg_management::Ctx {
        http: &http,
        sources: &sources,
        download_locks: &download_locks,
        addon_dir: &profile.addon_dir,
        cache_dir: &profile.global_config.dirs.cache,
        flavour: profile.product.flavour(),
    };

    let target = if args.addons.is_empty() {
        pkg_management::UpdateTarget::All
    } else {
        pkg_management::UpdateTarget::Specific(parse_defns(&args.addons, &sources)?)
    };

    let mut results = pkg_management::update(&mut lock, &pkg_ctx, target, args.dry_run).await;
    if args.addons.is_empty() {
        // Syncing "all": don't clutter output with already-up-to-date,
        // unpinned packages.
        results.retain(|_, r| {
            !matches!(
                r,
                Err(Failure::Manager(ManagerError::PkgUpToDate {
                    is_pinned: false
                }))
            )
        });
    }
    println!("{}", format_results(&results, style::color_enabled()));
    Ok(i32::from(any_errors(&results)))
}

async fn cmd_remove(cli: &Cli, args: &RemoveArgs) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
    let AppCtx {
        profile,
        mut lock,
        sources,
        ..
    } = app_ctx;
    let defns = parse_defns_retain(&args.addons, &sources)?;

    let results = pkg_management::remove(&mut lock, &profile.addon_dir, &defns, args.keep_folders);
    println!("{}", format_results(&results, style::color_enabled()));
    Ok(i32::from(any_errors(&results)))
}

/// Switches one installed addon to a different source. `old` is looked up
/// with `retain_unknown_source = true`, like `remove`, so it can still target
/// a package left behind by a since-removed/disabled source; `new` must
/// resolve against a live source since it drives a real download+install.
async fn cmd_replace(cli: &Cli, args: &ReplaceArgs) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
    let AppCtx {
        profile,
        mut lock,
        http,
        sources,
        download_locks,
    } = app_ctx;
    let old = parse_defn(&args.old, &sources, true)?;
    let new = parse_defn(&args.new, &sources, false)?;
    let pkg_ctx = pkg_management::Ctx {
        http: &http,
        sources: &sources,
        download_locks: &download_locks,
        addon_dir: &profile.addon_dir,
        cache_dir: &profile.global_config.dirs.cache,
        flavour: profile.product.flavour(),
    };

    let results = pkg_management::replace(&mut lock, &pkg_ctx, &[(old, new)]).await?;
    println!("{}", format_results(&results, style::color_enabled()));
    Ok(i32::from(any_errors(&results)))
}

// ============================================================================
// init
// ============================================================================

/// With `-p`/`--profile` given, reconciles just that one profile (name or
/// path, same resolution as every other command). Without it, reconciles
/// *every* configured profile (`ProfileConfig::iter_profiles`) in turn —
/// there's no implicit "default" to fall back to instead. Each profile is
/// otherwise handled exactly as a single-profile `init` always was: for
/// each group of untracked folders the matcher passes turn up, prompts
/// which candidate source to install (`--auto` picks the top-priority one
/// instead) then confirms before installing; folders that never match
/// anything are printed at the end for the user to handle by hand.
async fn cmd_init(cli: &Cli, args: &InitArgs) -> Result<i32, AppError> {
    let global = GlobalConfig::read_from(cli.config.as_deref())?;

    let targets: Vec<String> = match &cli.profile {
        Some(arg) => vec![arg.clone()],
        None => {
            let names = ProfileConfig::iter_profiles(&global);
            if names.is_empty() {
                println!(
                    "No profiles configured; hand-write one — see examples/config.toml and \
                     examples/profiles/example/profile.toml."
                );
                return Ok(0);
            }
            names
        }
    };

    // Not needed at all for `--list-unreconciled`, and shared across every
    // profile otherwise — fetched at most once, lazily, on the first profile
    // that actually turns out to have leftovers (so e.g. an all-clean set of
    // profiles never touches the network).
    let mut catalogue: Option<catalogue::ComputedCatalogue> = None;

    let mut any_errors = false;
    let print_headers = targets.len() > 1;
    for target in &targets {
        if print_headers {
            println!("== {target} ==");
        }
        if let Err(e) = reconcile_profile(&global, target, args, &mut catalogue).await {
            eprintln!("{target}: {e}");
            any_errors = true;
        }
    }
    Ok(i32::from(any_errors))
}

async fn reconcile_profile(
    global: &GlobalConfig,
    profile_arg: &str,
    args: &InitArgs,
    catalogue: &mut Option<catalogue::ComputedCatalogue>,
) -> Result<(), AppError> {
    let profile = ctx::read_profile(global.clone(), profile_arg)?;
    let AppCtx {
        profile,
        mut lock,
        http,
        sources,
        download_locks,
    } = AppCtx::from_profile(profile)?;
    let flavour = profile.product.flavour();

    let mut leftovers = matchers::get_unreconciled_folders(&lock, &profile.addon_dir, flavour);
    if args.list_unreconciled {
        print_unreconciled(&leftovers, style::color_enabled());
        return Ok(());
    }
    if leftovers.is_empty() {
        println!("No add-ons left to reconcile.");
        return Ok(());
    }

    if catalogue.is_none() {
        *catalogue = Some(catalogue::synchronise(&http).await?);
    }
    let catalogue = catalogue
        .as_ref()
        .expect("just populated above if it was None");

    let pkg_ctx = pkg_management::Ctx {
        http: &http,
        sources: &sources,
        download_locks: &download_locks,
        addon_dir: &profile.addon_dir,
        cache_dir: &profile.global_config.dirs.cache,
        flavour,
    };

    type Pass<'a> = Box<dyn Fn(&[matchers::AddonFolder]) -> Vec<matchers::MatcherGroup> + 'a>;
    let passes: Vec<Pass<'_>> = vec![
        Box::new(|leftovers| matchers::match_toc_source_ids(leftovers, catalogue, &sources)),
        Box::new(|leftovers| {
            matchers::match_folder_name_subsets(leftovers, flavour, catalogue, &sources)
        }),
        Box::new(|leftovers| matchers::match_addon_names_with_folder_names(leftovers, catalogue)),
    ];

    for pass in passes {
        let groups = pass(&leftovers);

        let mut all_defns: Vec<Defn> = Vec::new();
        for group in &groups {
            for d in &group.defns {
                if !all_defns.contains(d) {
                    all_defns.push(d.clone());
                }
            }
        }
        let resolve_results = pkg_management::resolve(&pkg_ctx, &all_defns, false).await;
        let (candidates, _errors) = pkg_management::split_results(resolve_results);

        let mut selections: Vec<Defn> = Vec::new();
        for group in &groups {
            let shortlist: Vec<(&Defn, &PkgCandidate)> = group
                .defns
                .iter()
                .filter_map(|d| candidates.get(d).map(|c| (d, c)))
                .collect();
            if shortlist.is_empty() {
                continue;
            }

            // `group.defns` is already priority-sorted, so the first
            // resolvable one is the same pick `--auto` makes explicitly.
            let selection = if args.auto {
                Some(shortlist[0].0.clone())
            } else {
                let names: Vec<&str> = group.folders.iter().map(|f| f.name.as_str()).collect();
                let version = group.folders.first().and_then(|f| f.toc.version.clone());
                let prompt = format!(
                    "{} [{}]",
                    names.join(", "),
                    version.as_deref().unwrap_or("?")
                );
                let choices: Vec<Choice<Defn>> = shortlist
                    .iter()
                    .map(|(d, c)| {
                        Choice::new(
                            format!("{} ({})", d.as_uri(false, false), c.version),
                            (*d).clone(),
                        )
                    })
                    .collect();
                prompts::select_one(&prompt, choices).ok()
            };
            if let Some(d) = selection {
                selections.push(d);
            }
        }

        if !selections.is_empty() {
            let proceed = args.auto || prompts::confirm("Install selected add-ons?", true)?;
            if proceed {
                let results =
                    pkg_management::install(&mut lock, &pkg_ctx, &selections, true, false).await;
                println!("{}", format_results(&results, style::color_enabled()));
            }
        }

        leftovers = matchers::get_unreconciled_folders(&lock, &profile.addon_dir, flavour);
        if leftovers.is_empty() {
            break;
        }
    }

    if !leftovers.is_empty() {
        println!();
        print_unreconciled(&leftovers, style::color_enabled());
    }

    Ok(())
}

fn print_unreconciled(leftovers: &[matchers::AddonFolder], color: bool) {
    if leftovers.is_empty() {
        println!("No add-ons left to reconcile.");
        return;
    }
    let mut names: Vec<&str> = leftovers.iter().map(|f| f.name.as_str()).collect();
    names.sort_unstable();
    println!("unreconciled:");
    for name in names {
        println!("  {}", style::name(color, name));
    }
}

// ============================================================================
// search
// ============================================================================

async fn cmd_search(cli: &Cli, args: &SearchArgs) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
    let AppCtx {
        profile,
        mut lock,
        http,
        sources,
        download_locks,
    } = app_ctx;
    let flavour = profile.product.flavour();

    let catalogue = catalogue::synchronise(&http).await?;
    let installed_keys: HashSet<(String, String)> = lock
        .get_all_pkgs()
        .into_iter()
        .map(|p| (p.source, p.id))
        .collect();

    let start_date = args
        .start_date
        .as_deref()
        .map(|s| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .map_err(|_| {
                    AppError::Other("invalid --start-date, expected YYYY-MM-DD".to_owned())
                })
                .map(|d| {
                    d.and_hms_opt(0, 0, 0)
                        .expect("midnight is always valid")
                        .and_utc()
                })
        })
        .transpose()?;

    let filter_installed = if args.no_exclude_installed {
        FilterInstalled::Ident
    } else {
        FilterInstalled::ExcludeFromAllSources
    };
    let options = SearchOptions {
        limit: args.limit as usize,
        sources: &args.sources,
        prefer_source: args.prefer_source.as_deref(),
        start_date,
        filter_installed,
    };
    let terms = args.terms.join(" ");
    let entries = search::search(
        &catalogue.entries,
        &terms,
        flavour,
        &installed_keys,
        &options,
    );

    if entries.is_empty() {
        println!("No results found.");
        return Ok(0);
    }

    let color = style::color_enabled();
    println!(
        "{}",
        crate::output::format_search_results(&entries, &installed_keys, color)
    );
    println!(
        "{} Packages to install (eg: 1 2 3, 1-3):",
        style::marker(color)
    );
    let input = prompts::read_line(&format!("{} ", style::marker(color)))?;

    let picked = parse_selection(&input, entries.len());
    if picked.is_empty() {
        println!("Nothing selected.");
        return Ok(0);
    }
    let selections: Vec<Defn> = picked
        .into_iter()
        .map(|i| {
            let e = entries[i];
            let alias = if e.slug.is_empty() {
                e.id.clone()
            } else {
                e.slug.clone()
            };
            Defn::new(e.source.clone(), alias)
        })
        .collect();

    let pkg_ctx = pkg_management::Ctx {
        http: &http,
        sources: &sources,
        download_locks: &download_locks,
        addon_dir: &profile.addon_dir,
        cache_dir: &profile.global_config.dirs.cache,
        flavour,
    };
    let results = pkg_management::install(&mut lock, &pkg_ctx, &selections, false, false).await;
    println!("{}", format_results(&results, style::color_enabled()));
    Ok(i32::from(any_errors(&results)))
}

// ============================================================================
// list / info
// ============================================================================

async fn cmd_list(cli: &Cli, addons: &[String], format: ListFormat) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
    let all = app_ctx.lock.get_all_pkgs();
    let mut pkgs = filter_pkgs_by_addons(&all, addons);
    pkgs.sort_by(|a, b| {
        (&a.source, a.name.to_lowercase()).cmp(&(&b.source, b.name.to_lowercase()))
    });

    let refs: Vec<&Pkg> = pkgs.iter().collect();
    let color = style::color_enabled();
    let rendered = match format {
        ListFormat::Simple => crate::output::format_list_simple(&refs, color),
        ListFormat::Detailed => crate::output::format_list_detailed(&refs, color),
        ListFormat::Json => crate::output::format_list_json(&refs),
    };
    if !rendered.is_empty() {
        println!("{rendered}");
    }
    Ok(0)
}

// ============================================================================
// profile
// ============================================================================

fn cmd_profile_erase(cli: &Cli) -> Result<(), AppError> {
    let global = GlobalConfig::read_from(cli.config.as_deref())?;
    let profile = ctx::resolve_profile(global, cli.profile.as_deref())
        .map_err(|e| describe_config_error(cli, e))?;
    profile.delete()?;
    Ok(())
}

// ============================================================================
// search selection parsing
// ============================================================================

/// Parses a paru-style selection line (`"1 2 3"`, `"1,2 4-6"`, ...) against
/// `max` results, returning 0-indexed positions into the original results
/// slice, deduplicated, in first-seen order. Tokens are split on whitespace
/// and commas; each is either a bare number or an inclusive `a-b` range
/// (either order). Out-of-range or unparseable tokens are silently dropped
/// rather than erroring the whole line out — matching how paru itself
/// tolerates a stray typo in an otherwise-valid selection.
fn parse_selection(input: &str, max: usize) -> Vec<usize> {
    let mut picked: Vec<usize> = Vec::new();
    let mut push_valid = |n: usize| {
        if n >= 1 && n <= max && !picked.contains(&(n - 1)) {
            picked.push(n - 1);
        }
    };

    for token in input.split([' ', ',']).map(str::trim) {
        if token.is_empty() {
            continue;
        }
        if let Some((a, b)) = token.split_once('-')
            && let (Ok(a), Ok(b)) = (a.parse::<usize>(), b.parse::<usize>())
        {
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            for n in lo..=hi {
                push_valid(n);
            }
        } else if let Ok(n) = token.parse::<usize>() {
            push_valid(n);
        }
    }

    picked
}

// ============================================================================
// Defn parsing / installed-package lookup helpers
// ============================================================================

/// Parses a CLI-supplied addon URI into a `Defn`: try [`Defn::from_uri`],
/// then fall back to each source's `get_alias_from_url` when the scheme is
/// unrecognized, else error out.
fn parse_defn(
    uri: &str,
    sources: &[Box<dyn Resolver>],
    retain_unknown_source: bool,
) -> Result<Defn, AppError> {
    let known: Vec<&str> = sources.iter().map(|r| r.metadata().id).collect();
    let defn = Defn::from_uri(uri, &known, retain_unknown_source)
        .map_err(|e| AppError::Other(e.to_string()))?;

    if defn.source.is_empty() {
        for resolver in sources {
            if let Some(alias) = resolver.get_alias_from_url(&defn.alias) {
                return Ok(Defn::new(resolver.metadata().id, alias));
            }
        }
        if !defn.alias.contains(':') {
            return Err(AppError::Other(format!(
                "could not determine addon source for '{uri}'"
            )));
        }
    }
    Ok(defn)
}

fn parse_defns(uris: &[String], sources: &[Box<dyn Resolver>]) -> Result<Vec<Defn>, AppError> {
    uris.iter().map(|u| parse_defn(u, sources, false)).collect()
}

fn parse_defns_retain(
    uris: &[String],
    sources: &[Box<dyn Resolver>],
) -> Result<Vec<Defn>, AppError> {
    uris.iter().map(|u| parse_defn(u, sources, true)).collect()
}

/// Filters already-installed packages by a list of raw CLI arguments: each
/// arg is either a `source:ident` pair (exact id/slug match within that
/// source) or a bare string (case-insensitive substring match against the
/// slug). Empty `addons` means "every installed package". Filters in memory
/// over the small installed-package set rather than generating a SQL
/// `WHERE` clause.
fn filter_pkgs_by_addons(all: &[Pkg], addons: &[String]) -> Vec<Pkg> {
    if addons.is_empty() {
        return all.to_vec();
    }

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for raw in addons {
        for pkg in all {
            let key = (pkg.source.clone(), pkg.id.clone());
            if seen.contains(&key) {
                continue;
            }
            let matched = match raw.split_once(':') {
                Some((source, ident)) => {
                    pkg.source == source
                        && (pkg.id == ident || pkg.slug.eq_ignore_ascii_case(ident))
                }
                None => pkg.id == *raw || pkg.slug.to_lowercase().contains(&raw.to_lowercase()),
            };
            if matched {
                seen.insert(key);
                out.push(pkg.clone());
            }
        }
    }
    out
}
