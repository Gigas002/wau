//! Command dispatch onto the `libwau` API assembled by [`crate::ctx`]. No
//! addon logic lives here beyond `Defn`/installed-package lookup glue;
//! every actual operation delegates to `libwau`.

use std::{collections::HashSet, path::PathBuf, time::Duration};

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
    model::{Defn, Flavour, infer_product_from_addon_dir},
    pkg_management,
    results::{Failure, ManagerError},
    sources::{PkgCandidate, Resolver},
};

use serde::Serialize;

use crate::{
    cli::{
        CacheCommand, Cli, Command, InitArgs, InstallArgs, ListFormat, ProfileCommand,
        RemoveArgs, SearchArgs, SyncArgs,
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
        Command::Sync(args) => cmd_sync(cli, args).await,
        Command::Remove(args) => cmd_remove(cli, args).await,
        Command::Init(args) => cmd_init(cli, args).await,
        Command::Search(args) => cmd_search(cli, args).await,
        Command::List(args) => cmd_list(cli, &args.addons, args.format).await,
        Command::Info(args) => {
            cmd_list(cli, std::slice::from_ref(&args.addon), ListFormat::Detailed).await
        }
        Command::Cache(CacheCommand::Clear) => {
            cmd_cache_clear().await?;
            Ok(0)
        }
        Command::Profile(ProfileCommand::Erase) => {
            cmd_profile_erase(&cli.profile)?;
            Ok(0)
        }
        Command::Stats => {
            cmd_stats(cli)?;
            Ok(0)
        }
    }
}

// ============================================================================
// Ctx bootstrap
// ============================================================================

/// Builds the [`AppCtx`] for `cli.profile`. Every command except `init` calls
/// this: if the profile isn't configured yet, it errors out pointing at
/// `wau init` and the example TOML configs, rather than prompting — only
/// `init` bootstraps a profile interactively (see [`ensure_ctx_bootstrap`]).
fn load_ctx(cli: &Cli) -> Result<AppCtx, AppError> {
    AppCtx::build(&cli.profile, cli.no_cache).map_err(|e| match e {
        CtxError::Config(ConfigError::NotFound { path }) => AppError::Other(format!(
            "profile '{}' isn't configured (expected {}); run `wau init` to set it up \
             interactively, or hand-write it — see examples/config.toml and \
             examples/profiles/example.toml",
            cli.profile,
            path.display()
        )),
        e => e.into(),
    })
}

/// Builds the [`AppCtx`] for `cli.profile`, interactively bootstrapping it
/// first if it hasn't been configured yet. Only [`cmd_init`] uses this;
/// every other command uses [`load_ctx`] and errors out instead.
async fn ensure_ctx_bootstrap(cli: &Cli) -> Result<AppCtx, AppError> {
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
// install / sync / remove
// ============================================================================

async fn cmd_install(cli: &Cli, args: &InstallArgs) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
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

async fn cmd_sync(cli: &Cli, args: &SyncArgs) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
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
    println!("{}", format_results(&results));
    Ok(i32::from(any_errors(&results)))
}

async fn cmd_remove(cli: &Cli, args: &RemoveArgs) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
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
// init
// ============================================================================

async fn cmd_init(cli: &Cli, args: &InitArgs) -> Result<i32, AppError> {
    let app_ctx = ensure_ctx_bootstrap(cli).await?;
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

// ============================================================================
// search
// ============================================================================

async fn cmd_search(cli: &Cli, args: &SearchArgs) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
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
// list / info
// ============================================================================

async fn cmd_list(cli: &Cli, addons: &[String], format: ListFormat) -> Result<i32, AppError> {
    let app_ctx = load_ctx(cli)?;
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

// ============================================================================
// cache / profile / stats
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

#[derive(Serialize)]
struct StatsOutput {
    active_profile: String,
    profiles: Vec<String>,
    active_profile_config: Option<StatsProfileConfig>,
    global_config: StatsGlobalConfig,
    sources: Vec<StatsSourceMeta>,
}

#[derive(Serialize)]
struct StatsProfileConfig {
    addon_dir: PathBuf,
    flavour: Flavour,
}

#[derive(Serialize)]
struct StatsGlobalConfig {
    log_level: libwau::config::LogLevel,
    dirs: libwau::config::Dirs,
}

#[derive(Serialize)]
struct StatsSourceMeta {
    #[serde(flatten)]
    metadata: libwau::model::SourceMetadata,
    disabled_reason: Option<String>,
}

fn cmd_stats(cli: &Cli) -> Result<(), AppError> {
    let global = GlobalConfig::read()?;
    let profiles = ProfileConfig::iter_profiles(&global);
    let active_profile_config = ProfileConfig::read(global.clone(), &cli.profile).ok();
    let sources = libwau::sources::default_sources(&ctx::source_config(&global));

    let source_meta: Vec<StatsSourceMeta> = sources
        .iter()
        .map(|r| StatsSourceMeta {
            metadata: r.metadata(),
            disabled_reason: r.get_disabled_reason(),
        })
        .collect();

    let output = StatsOutput {
        active_profile: cli.profile.clone(),
        profiles,
        active_profile_config: active_profile_config.map(|p| StatsProfileConfig {
            addon_dir: p.addon_dir,
            flavour: p.product.flavour(),
        }),
        global_config: StatsGlobalConfig {
            log_level: global.log_level,
            dirs: global.dirs,
        },
        sources: source_meta,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_default()
    );
    Ok(())
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
