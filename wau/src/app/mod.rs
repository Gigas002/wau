//! Command dispatch — ports the command bodies of instawow's `cli/__init__.py`
//! onto the `libwau` API assembled by [`crate::ctx`]. No addon logic lives
//! here beyond `Defn`/installed-package lookup glue; every actual operation
//! delegates to `libwau`.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    time::Duration,
};

use libwau::{
    catalogue::{
        self,
        search::{self, FilterInstalled, SearchOptions},
    },
    config::{ConfigError, GlobalConfig, ProfileConfig, SecretString},
    db::{self, Pkg},
    github_auth::GitHubAuth,
    http::HttpClient,
    matchers,
    model::{Defn, Flavour, Strategies, infer_product_from_addon_dir},
    pkg_management,
    results::{Failure, ManagerError},
    sources::{PkgCandidate, Resolver, find_source},
};
use rusqlite::Connection;

use crate::{
    cli::{
        CacheCommand, Cli, Command, InstallArgs, ListFormat, ProfileCommand, ReconcileArgs,
        RemoveArgs, RereconcileArgs, RevealArgs, RollbackArgs, SearchArgs, UpdateArgs,
        ViewChangelogArgs,
    },
    ctx::{self, AppCtx, CtxError},
    output::{any_errors, format_results},
    prompts::{self, Choice},
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
    Db(#[from] db::DbError),
    #[error(transparent)]
    Http(#[from] libwau::http::HttpError),
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Failure(#[from] Failure),
    #[error(transparent)]
    Prompt(#[from] inquire::InquireError),
    #[error("{0}")]
    Other(String),
}

pub async fn run(cli: &Cli) -> Result<i32, AppError> {
    match &cli.command {
        Command::Install(args) => cmd_install(cli, args).await,
        Command::Update(args) => cmd_update(cli, args).await,
        Command::Remove(args) => cmd_remove(cli, args).await,
        Command::Rollback(args) => cmd_rollback(cli, args).await,
        Command::Reconcile(args) => cmd_reconcile(cli, args).await,
        Command::Rereconcile(args) => cmd_rereconcile(cli, args).await,
        Command::Search(args) => cmd_search(cli, args).await,
        Command::List(args) => cmd_list(cli, &args.addons, args.format).await,
        Command::Info(args) => {
            cmd_list(cli, std::slice::from_ref(&args.addon), ListFormat::Detailed).await
        }
        Command::Reveal(args) => cmd_reveal(cli, args).await,
        Command::ViewChangelog(args) => cmd_view_changelog(cli, args).await,
        Command::Cache(CacheCommand::Clear) => {
            cmd_cache_clear().await?;
            Ok(0)
        }
        Command::Profile(ProfileCommand::Configure(args)) => {
            cmd_configure(&args.options, &cli.profile).await?;
            Ok(0)
        }
        Command::Profile(ProfileCommand::Erase) => {
            cmd_profile_erase(&cli.profile)?;
            Ok(0)
        }
        Command::Configure(args) => {
            cmd_configure(&args.options, &cli.profile).await?;
            Ok(0)
        }
        Command::Debug => {
            cmd_debug(cli)?;
            Ok(0)
        }
    }
}

// ============================================================================
// Ctx bootstrap
// ============================================================================

/// Builds the [`AppCtx`] for `cli.profile`, interactively bootstrapping it
/// first if it hasn't been configured yet — mirrors instawow's
/// `Group.invoke`/`_perform_installation_check` fallback into
/// `configure_profile` on an unconfigured profile.
async fn ensure_ctx(cli: &Cli) -> Result<AppCtx, AppError> {
    match AppCtx::build(&cli.profile, cli.no_cache) {
        Ok(app_ctx) => Ok(app_ctx),
        Err(CtxError::Config(ConfigError::NotFound { .. })) => {
            println!("Profile '{}' isn't configured yet.", cli.profile);
            let mut global = GlobalConfig::read()?;
            let profile = configure_profile_interactive(&mut global, &cli.profile).await?;
            Ok(AppCtx::from_profile(profile, cli.no_cache)?)
        }
        Err(e) => Err(e.into()),
    }
}

async fn configure_profile_interactive(
    global: &mut GlobalConfig,
    profile_name: &str,
) -> Result<ProfileConfig, AppError> {
    let addon_dir_input = prompts::text("Add-on directory:")?;
    let addon_dir = PathBuf::from(addon_dir_input.trim());

    let flavour_override = if infer_product_from_addon_dir(&addon_dir).is_none() {
        let choices: Vec<Choice<Flavour>> = Flavour::ALL
            .iter()
            .map(|f| Choice::new(f.to_string(), *f))
            .collect();
        Some(prompts::select_one("Game flavour", choices)?)
    } else {
        None
    };

    global.auto_update_check = prompts::confirm("Periodically check for wau updates?", true)?;

    if global.access_tokens.github.is_none()
        && prompts::confirm("Set up GitHub authentication?", false)?
    {
        match run_github_oauth_flow().await {
            Ok(token) => global.access_tokens.github = Some(SecretString::new(token)),
            Err(e) => println!("GitHub authentication failed: {e}"),
        }
    }

    if global.access_tokens.cfcore.is_none() {
        println!(
            "An API key is required to use CurseForge. Log in to CurseForge for Studios \
             <https://console.curseforge.com/> to generate a key."
        );
        let key = prompts::password("CurseForge API key (leave blank to skip):")?;
        if !key.is_empty() {
            global.access_tokens.cfcore = Some(SecretString::new(key));
        }
    }

    if global.access_tokens.wago_addons.is_none() {
        println!(
            "An access token is required to use Wago Addons. Wago issues tokens to Patreon \
             <https://addons.wago.io/patreon> subscribers above a certain tier."
        );
        let token = prompts::password("Wago Addons access token (leave blank to skip):")?;
        if !token.is_empty() {
            global.access_tokens.wago_addons = Some(SecretString::new(token));
        }
    }

    global.write()?;

    let profile = ProfileConfig::new(global.clone(), profile_name, addon_dir, flavour_override)?;
    profile.write()?;
    Ok(profile)
}

async fn run_github_oauth_flow() -> Result<String, AppError> {
    let http = HttpClient::new(None)?;
    let auth = GitHubAuth::new();
    let codes = auth.get_codes(&http).await?;
    println!(
        "Navigate to {} and paste the code below:",
        codes.verification_uri
    );
    println!("  {}", codes.user_code);
    println!("Waiting...");
    let token = auth
        .poll_for_access_token(
            &http,
            &codes.device_code,
            Duration::from_secs(codes.interval),
        )
        .await?;
    Ok(token)
}

// ============================================================================
// install / update / remove
// ============================================================================

async fn cmd_install(cli: &Cli, args: &InstallArgs) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx(cli).await?;
    let AppCtx {
        profile,
        mut conn,
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
        pkg_management::install(&mut conn, &pkg_ctx, &defns, args.replace, args.dry_run).await;
    println!("{}", format_results(&results));
    Ok(i32::from(any_errors(&results)))
}

async fn cmd_update(cli: &Cli, args: &UpdateArgs) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx(cli).await?;
    let AppCtx {
        profile,
        mut conn,
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

    let mut results = pkg_management::update(&mut conn, &pkg_ctx, target, args.dry_run).await;
    if args.addons.is_empty() {
        // Updating "all": don't clutter output with already-up-to-date,
        // unpinned packages — matches instawow's `update` command filter.
        results.retain(|_, r| {
            !matches!(
                r,
                Err(Failure::Manager(ManagerError::PkgUpToDate {
                    is_pinned: false
                }))
            )
        });
    }
    println!("{}", format_results(&results));
    Ok(i32::from(any_errors(&results)))
}

async fn cmd_remove(cli: &Cli, args: &RemoveArgs) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx(cli).await?;
    let AppCtx {
        profile,
        mut conn,
        sources,
        ..
    } = app_ctx;
    let defns = parse_defns_retain(&args.addons, &sources)?;

    let results = pkg_management::remove(&mut conn, &profile.addon_dir, &defns, args.keep_folders);
    println!("{}", format_results(&results));
    Ok(i32::from(any_errors(&results)))
}

// ============================================================================
// rollback
// ============================================================================

async fn cmd_rollback(cli: &Cli, args: &RollbackArgs) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx(cli).await?;
    let AppCtx {
        profile,
        mut conn,
        http,
        sources,
        download_locks,
    } = app_ctx;
    let defn = parse_defn(&args.addon, &sources, false)?;

    let Some(pkg) = db::get_pkgs(&conn, std::slice::from_ref(&defn))?
        .into_iter()
        .next()
        .flatten()
    else {
        let results = HashMap::from([(defn, Err(ManagerError::PkgNotInstalled.into()))]);
        println!("{}", format_results(&results));
        return Ok(1);
    };

    let pkg_ctx = pkg_management::Ctx {
        http: &http,
        sources: &sources,
        download_locks: &download_locks,
        addon_dir: &profile.addon_dir,
        cache_dir: &profile.global_config.dirs.cache,
        flavour: profile.product.flavour(),
    };

    if args.undo {
        let mut cleared = defn;
        cleared.strategies = Strategies::default();
        let results = pkg_management::update(
            &mut conn,
            &pkg_ctx,
            pkg_management::UpdateTarget::Specific(vec![cleared]),
            false,
        )
        .await;
        println!("{}", format_results(&results));
        return Ok(i32::from(any_errors(&results)));
    }

    let logged = db::get_pkg_logged_versions(&conn, &pkg.source, &pkg.id)?;
    if logged.len() <= 1 {
        let results = HashMap::from([(
            defn,
            Err(ManagerError::PkgFilesMissing {
                reason: "cannot find older versions".to_owned(),
            }
            .into()),
        )]);
        println!("{}", format_results(&results));
        return Ok(1);
    }

    let reconstructed = pkg.to_defn();
    let choices: Vec<Choice<String>> = logged
        .iter()
        .map(|v| {
            let label = if v.version == pkg.version {
                format!("{} (current)", v.version)
            } else {
                v.version.clone()
            };
            Choice::new(label, v.version.clone())
        })
        .collect();
    let selected = prompts::select_one(
        &format!(
            "Select version of {} for rollback",
            reconstructed.as_uri(false, false)
        ),
        choices,
    )?;

    let target = reconstructed.with_version(selected);
    let results = pkg_management::update(
        &mut conn,
        &pkg_ctx,
        pkg_management::UpdateTarget::Specific(vec![target]),
        false,
    )
    .await;
    println!("{}", format_results(&results));
    Ok(i32::from(any_errors(&results)))
}

// ============================================================================
// reconcile / rereconcile
// ============================================================================

async fn cmd_reconcile(cli: &Cli, args: &ReconcileArgs) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx(cli).await?;
    let AppCtx {
        profile,
        mut conn,
        http,
        sources,
        download_locks,
    } = app_ctx;
    let flavour = profile.product.flavour();

    let mut leftovers = matchers::get_unreconciled_folders(&conn, &profile.addon_dir, flavour);
    if args.list_unreconciled {
        print_unreconciled(&leftovers);
        return Ok(0);
    }
    if leftovers.is_empty() {
        println!("No add-ons left to reconcile.");
        return Ok(0);
    }

    let catalogue = catalogue::synchronise(&http).await?;
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
        Box::new(|leftovers| matchers::match_toc_source_ids(leftovers, &catalogue, &sources)),
        Box::new(|leftovers| {
            matchers::match_folder_name_subsets(leftovers, flavour, &catalogue, &sources)
        }),
        Box::new(|leftovers| matchers::match_addon_names_with_folder_names(leftovers, &catalogue)),
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
                    pkg_management::install(&mut conn, &pkg_ctx, &selections, true, false).await;
                println!("{}", format_results(&results));
            }
        }

        leftovers = matchers::get_unreconciled_folders(&conn, &profile.addon_dir, flavour);
        if leftovers.is_empty() {
            break;
        }
    }

    if !leftovers.is_empty() {
        println!();
        print_unreconciled(&leftovers);
    }

    Ok(0)
}

fn print_unreconciled(leftovers: &[matchers::AddonFolder]) {
    if leftovers.is_empty() {
        println!("No add-ons left to reconcile.");
        return;
    }
    let mut names: Vec<&str> = leftovers.iter().map(|f| f.name.as_str()).collect();
    names.sort_unstable();
    println!("unreconciled:");
    for name in names {
        println!("  {name}");
    }
}

async fn cmd_rereconcile(cli: &Cli, args: &RereconcileArgs) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx(cli).await?;
    let AppCtx {
        profile,
        mut conn,
        http,
        sources,
        download_locks,
    } = app_ctx;
    let flavour = profile.product.flavour();

    let all_pkgs = db::get_all_pkgs(&conn)?;
    let pkgs = filter_pkgs_by_addons(&all_pkgs, &args.addons);

    let catalogue = catalogue::synchronise(&http).await?;
    let equivalents = matchers::find_equivalent_pkg_defns(
        &pkgs,
        &profile.addon_dir,
        flavour,
        &catalogue,
        &sources,
    );
    if equivalents.is_empty() {
        println!("No equivalent add-ons found.");
        return Ok(0);
    }

    let pkg_ctx = pkg_management::Ctx {
        http: &http,
        sources: &sources,
        download_locks: &download_locks,
        addon_dir: &profile.addon_dir,
        cache_dir: &profile.global_config.dirs.cache,
        flavour,
    };

    let mut all_defns: Vec<Defn> = Vec::new();
    for defns in equivalents.values() {
        for d in defns {
            if !all_defns.contains(d) {
                all_defns.push(d.clone());
            }
        }
    }
    let resolve_results = pkg_management::resolve(&pkg_ctx, &all_defns, false).await;
    let (candidates, _errors) = pkg_management::split_results(resolve_results);

    let mut selections: Vec<(Defn, Defn)> = Vec::new();
    for pkg in &pkgs {
        let key = (pkg.source.clone(), pkg.id.clone());
        let Some(equiv_defns) = equivalents.get(&key) else {
            continue;
        };
        let shortlist: Vec<(&Defn, &PkgCandidate)> = equiv_defns
            .iter()
            .filter_map(|d| candidates.get(d).map(|c| (d, c)))
            .collect();
        if shortlist.is_empty() {
            continue;
        }

        let mut choices: Vec<Choice<Option<Defn>>> = vec![Choice::new(
            format!("{} (current)", pkg.to_defn().as_uri(false, false)),
            None,
        )];
        choices.extend(shortlist.iter().map(|(d, c)| {
            Choice::new(
                format!("{} ({})", d.as_uri(false, false), c.version),
                Some((*d).clone()),
            )
        }));

        if let Ok(Some(new_defn)) = prompts::select_one(&pkg.name, choices) {
            selections.push((pkg.to_defn(), new_defn));
        }
    }

    if selections.is_empty() {
        return Ok(0);
    }
    if !prompts::confirm("Install selected add-ons?", true)? {
        return Ok(0);
    }

    let results = pkg_management::replace(&mut conn, &pkg_ctx, &selections).await?;
    println!("{}", format_results(&results));
    Ok(i32::from(any_errors(&results)))
}

// ============================================================================
// search
// ============================================================================

async fn cmd_search(cli: &Cli, args: &SearchArgs) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx(cli).await?;
    let AppCtx {
        profile,
        mut conn,
        http,
        sources,
        download_locks,
    } = app_ctx;
    let flavour = profile.product.flavour();

    let catalogue = catalogue::synchronise(&http).await?;
    let installed_keys: HashSet<(String, String)> = db::get_all_pkgs(&conn)?
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

    let choices: Vec<Choice<Defn>> = entries
        .iter()
        .map(|e| {
            let alias = if e.slug.is_empty() {
                e.id.clone()
            } else {
                e.slug.clone()
            };
            let defn = Defn::new(e.source.clone(), alias);
            Choice::new(format!("{}  ({})", e.name, defn.as_uri(false, false)), defn)
        })
        .collect();

    let selections = prompts::select_multiple("Select add-ons to install", choices)?;
    if selections.is_empty() {
        println!(
            "Nothing was selected; select add-ons with <space> and confirm by pressing <enter>."
        );
        return Ok(0);
    }
    if !prompts::confirm("Install selected add-ons?", true)? {
        return Ok(0);
    }

    let pkg_ctx = pkg_management::Ctx {
        http: &http,
        sources: &sources,
        download_locks: &download_locks,
        addon_dir: &profile.addon_dir,
        cache_dir: &profile.global_config.dirs.cache,
        flavour,
    };
    let results = pkg_management::install(&mut conn, &pkg_ctx, &selections, false, false).await;
    println!("{}", format_results(&results));
    Ok(i32::from(any_errors(&results)))
}

// ============================================================================
// list / info / reveal / view-changelog
// ============================================================================

async fn cmd_list(cli: &Cli, addons: &[String], format: ListFormat) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx(cli).await?;
    let all = db::get_all_pkgs(&app_ctx.conn)?;
    let mut pkgs = filter_pkgs_by_addons(&all, addons);
    pkgs.sort_by(|a, b| {
        (&a.source, a.name.to_lowercase()).cmp(&(&b.source, b.name.to_lowercase()))
    });

    let refs: Vec<&Pkg> = pkgs.iter().collect();
    let rendered = match format {
        ListFormat::Simple => crate::output::format_list_simple(&refs),
        ListFormat::Detailed => crate::output::format_list_detailed(&refs),
        ListFormat::Json => crate::output::format_list_json(&refs),
    };
    if !rendered.is_empty() {
        println!("{rendered}");
    }
    Ok(0)
}

async fn cmd_reveal(cli: &Cli, args: &RevealArgs) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx(cli).await?;
    let all = db::get_all_pkgs(&app_ctx.conn)?;
    let pkgs = filter_pkgs_by_addons(&all, std::slice::from_ref(&args.addon));

    let Some(pkg) = pkgs.into_iter().next() else {
        let defn = Defn::new(String::new(), args.addon.clone());
        let results = HashMap::from([(defn, Err(ManagerError::PkgNotInstalled.into()))]);
        println!("{}", format_results(&results));
        return Ok(1);
    };
    let Some(folder) = pkg.folders.first() else {
        return Ok(1);
    };

    reveal_path(&app_ctx.profile.addon_dir.join(&folder.name));
    Ok(0)
}

fn reveal_path(path: &std::path::Path) {
    #[cfg(target_os = "macos")]
    let _ = std::process::Command::new("open").arg(path).status();
    #[cfg(target_os = "windows")]
    let _ = std::process::Command::new("explorer").arg(path).status();
    #[cfg(all(unix, not(target_os = "macos")))]
    let _ = std::process::Command::new("xdg-open").arg(path).status();
}

struct ChangelogEntry {
    source: String,
    slug: String,
    changelog_url: String,
}

async fn cmd_view_changelog(cli: &Cli, args: &ViewChangelogArgs) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx(cli).await?;
    let AppCtx {
        profile,
        conn,
        http,
        sources,
        download_locks,
    } = app_ctx;
    let flavour = profile.product.flavour();

    let entries: Vec<ChangelogEntry> = if args.remote {
        let defns = parse_defns(&args.addons, &sources)?;
        let pkg_ctx = pkg_management::Ctx {
            http: &http,
            sources: &sources,
            download_locks: &download_locks,
            addon_dir: &profile.addon_dir,
            cache_dir: &profile.global_config.dirs.cache,
            flavour,
        };
        let resolve_results = pkg_management::resolve(&pkg_ctx, &defns, false).await;
        let (candidates, errors) = pkg_management::split_results(resolve_results);
        if !errors.is_empty() {
            let error_results: HashMap<Defn, Result<pkg_management::Outcome, Failure>> =
                errors.into_iter().map(|(d, e)| (d, Err(e))).collect();
            println!("{}", format_results(&error_results));
        }
        candidates
            .into_iter()
            .map(|(d, c)| ChangelogEntry {
                source: d.source,
                slug: c.slug,
                changelog_url: c.changelog_url,
            })
            .collect()
    } else {
        let all = db::get_all_pkgs(&conn)?;
        let pkgs = if args.addons.is_empty() {
            recently_installed_pkgs(&conn, &all)?
        } else {
            filter_pkgs_by_addons(&all, &args.addons)
        };
        pkgs.into_iter()
            .map(|p| ChangelogEntry {
                source: p.source,
                slug: p.slug,
                changelog_url: p.changelog_url,
            })
            .collect()
    };

    let mut blocks = Vec::new();
    for entry in &entries {
        let Some(resolver) = find_source(&sources, &entry.source) else {
            continue;
        };
        let changelog = match resolver.get_changelog(&http, &entry.changelog_url).await {
            Ok(c) => c,
            Err(e) => e.to_string(),
        };
        let lines: Vec<&str> = changelog.lines().take(100).collect();
        let body = lines
            .iter()
            .map(|l| format!("  {l}"))
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(format!(
            "{}:\n{body}",
            Defn::new(entry.source.clone(), entry.slug.clone()).as_uri(false, false)
        ));
    }

    if !blocks.is_empty() {
        println!("{}", blocks.join("\n\n"));
    }
    Ok(0)
}

/// Packages installed within one minute of the most recently installed
/// package — instawow's default `view-changelog` scope when no addons are
/// named. A raw query since it's a one-off, app-level lookup rather than
/// something the rest of the app needs from `libwau::db`.
fn recently_installed_pkgs(conn: &Connection, all: &[Pkg]) -> Result<Vec<Pkg>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT pkg_source, pkg_id FROM pkg_version_log \
         WHERE install_time >= (SELECT datetime(max(install_time), '-1 minute') FROM pkg_version_log)",
    )?;
    let keys: HashSet<(String, String)> = stmt
        .query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(all
        .iter()
        .filter(|p| keys.contains(&(p.source.clone(), p.id.clone())))
        .cloned()
        .collect())
}

// ============================================================================
// cache / profile / configure / debug
// ============================================================================

async fn cmd_cache_clear() -> Result<(), AppError> {
    let global = GlobalConfig::read()?;
    let http = HttpClient::new(Some(&global.dirs.cache))?;
    http.clear_cache().await?;
    Ok(())
}

fn cmd_profile_erase(profile_name: &str) -> Result<(), AppError> {
    let global = GlobalConfig::read()?;
    let profile = ProfileConfig::read(global, profile_name)?;
    profile.delete()?;
    Ok(())
}

async fn cmd_configure(options: &[String], profile_name: &str) -> Result<(), AppError> {
    let mut global = GlobalConfig::read()?;

    if options.is_empty() {
        let profile = configure_profile_interactive(&mut global, profile_name).await?;
        println!(
            "Configured profile '{}' -> {}",
            profile.profile,
            profile.addon_dir.display()
        );
        return Ok(());
    }

    let mut kv: HashMap<&str, &str> = HashMap::new();
    for opt in options {
        let (k, v) = opt
            .split_once('=')
            .ok_or_else(|| AppError::Other(format!("expected key=value, got '{opt}'")))?;
        kv.insert(k, v);
    }

    if let Some(v) = kv.get("auto_update_check") {
        global.auto_update_check = parse_bool(v)?;
    }
    if let Some(v) = kv.get("github_token") {
        global.access_tokens.github = Some(SecretString::new((*v).to_owned()));
    }
    if let Some(v) = kv.get("cfcore_api_key") {
        global.access_tokens.cfcore = Some(SecretString::new((*v).to_owned()));
    }
    if let Some(v) = kv.get("wago_addons_token") {
        global.access_tokens.wago_addons = Some(SecretString::new((*v).to_owned()));
    }
    global.write()?;

    let addon_dir = kv.get("addon_dir").map(PathBuf::from);
    let flavour_override = kv
        .get("flavour_override")
        .map(|v| Flavour::parse(v).ok_or_else(|| AppError::Other(format!("unknown flavour '{v}'"))))
        .transpose()?;

    if addon_dir.is_some() || flavour_override.is_some() {
        let existing = ProfileConfig::read(global.clone(), profile_name).ok();
        let addon_dir = addon_dir
            .or_else(|| existing.as_ref().map(|p| p.addon_dir.clone()))
            .ok_or_else(|| {
                AppError::Other("addon_dir is required to configure a new profile".to_owned())
            })?;
        let flavour_override =
            flavour_override.or_else(|| existing.as_ref().and_then(|p| p.flavour_override));
        let profile = ProfileConfig::new(global, profile_name, addon_dir, flavour_override)?;
        profile.write()?;
    }

    Ok(())
}

fn parse_bool(s: &str) -> Result<bool, AppError> {
    match s.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Ok(true),
        "0" | "false" | "no" | "off" => Ok(false),
        _ => Err(AppError::Other(format!("expected a boolean, got '{s}'"))),
    }
}

fn cmd_debug(cli: &Cli) -> Result<(), AppError> {
    let global = GlobalConfig::read()?;
    let profiles = ProfileConfig::iter_profiles(&global);
    let active_profile_config = ProfileConfig::read(global.clone(), &cli.profile).ok();
    let sources = libwau::sources::default_sources(&ctx::source_config(&global));

    let source_meta: Vec<serde_json::Value> = sources
        .iter()
        .map(|r| {
            let m = r.metadata();
            serde_json::json!({
                "id": m.id,
                "name": m.name,
                "strategies": m.strategies.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                "changelog_format": format!("{:?}", m.changelog_format),
                "addon_toc_key": m.addon_toc_key,
                "disabled_reason": r.get_disabled_reason(),
            })
        })
        .collect();

    let output = serde_json::json!({
        "active_profile": cli.profile,
        "profiles": profiles,
        "active_profile_config": active_profile_config.map(|p| serde_json::json!({
            "addon_dir": p.addon_dir,
            "flavour": p.product.flavour().to_string(),
        })),
        "global_config": {
            "auto_update_check": global.auto_update_check,
            "dirs": {
                "config": global.dirs.config,
                "cache": global.dirs.cache,
                "state": global.dirs.state,
            },
        },
        "sources": source_meta,
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_default()
    );
    Ok(())
}

// ============================================================================
// Defn parsing / installed-package lookup helpers
// ============================================================================

/// Parses a CLI-supplied addon URI into a `Defn` — ports
/// `_parse_defn_uri_option`'s core: try [`Defn::from_uri`], then fall back to
/// each source's `get_alias_from_url` when the scheme is unrecognized, else
/// error out (instawow's `raise_invalid=True` path).
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
/// slug). Empty `addons` means "every installed package". A simplification
/// of instawow's SQL `LIKE`-based `_make_pkg_where_clause_and_params`
/// (in-memory filtering over the small installed-package set instead of a
/// generated `WHERE` clause) — functionally equivalent for typical package counts.
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
