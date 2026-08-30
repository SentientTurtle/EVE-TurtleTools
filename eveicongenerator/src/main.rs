pub const CRATE_NAME: &'static str = env!("CARGO_PKG_NAME");
pub const CRATE_VERSION: &'static str = env!("CARGO_PKG_VERSION");
pub const CRATE_REPO: &'static str = env!("CARGO_PKG_REPOSITORY");

use crate::icons::{IconBuildData, IconConfig, IconError, OutputMode};
use evesharedcache::cache::CacheDownloader;
use std::time::Instant;
use fs_err as fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use clap::{Arg, ArgAction, Command};
use clap::builder::ValueParser;
use std::io::Write;
use evestaticdata::sde::load::SDELoader;

pub mod icons;

static LOG_FILE: OnceLock<fs::File> = OnceLock::new();

fn main() {
    match do_main() {
        Ok(()) => {}
        Err(err) => {
            println!("Error: {}", err);
            let log_file = LOG_FILE.get();
            if let Some(mut log) = log_file {
                writeln!(log, "Error: {}", err).unwrap();
            }
        }
    }
}

fn do_main() -> Result<(), IconError> {
    let arg_matches = Command::new("eveicongenerator")
        .about("Multi-purpose item-icon generator for EVE Online")
        .args([
            Arg::new("user_agent")
                .short('u')
                .long("user_agent")
                .help("User Agent for HTTP requests")
                .required_unless_present("user_agent_file")
                .conflicts_with("user_agent_file")
                .value_parser(ValueParser::string()),
            Arg::new("user_agent_file")
                .long("user_agent_file")
                .help("File containing User Agent for HTTP requests")
                .required_unless_present("user_agent")
                .conflicts_with("user_agent")
                .value_parser(ValueParser::path_buf()),
            Arg::new("cache_folder")
                .short('c')
                .long("cache_folder")
                .help("Game data cache folder to use")
                .default_value("./cache")
                .value_parser(ValueParser::path_buf()),
            Arg::new("icon_folder")
                .short('i')
                .long("icon_folder")
                .help("Output/Cache folder for icons")
                .default_value("./icons")
                .value_parser(ValueParser::path_buf()),
            Arg::new("logfile")
                .short('l')
                .long("logfile")
                .help("Log file to use, no logging if unset")
                .value_parser(ValueParser::path_buf()),
            Arg::new("append_log")
                .long("append_log")
                .help("Append to log file, if unset replaces log file")
                .requires("logfile")
                .action(ArgAction::SetTrue),
            Arg::new("silent")
                .long("silent")
                .help("Silent mode")
                .action(ArgAction::SetTrue),
            Arg::new("force_rebuild")
                .short('f')
                .long("force_rebuild")
                .help("Force-rebuild of unchanged icons")
                .action(ArgAction::SetTrue),
            Arg::new("skip_if_fresh")
                .short('s')
                .long("skip_if_fresh")
                .help("If icons are unchanged, skip output")
                .action(ArgAction::SetTrue),
            Arg::new("old_overlays")
                .long("old_overlays")
                .help("Use the old-style glossy overlays for tech-tiers & other info")
                .action(ArgAction::SetTrue),
            Arg::new("module_overlays")
                .long("module_overlays")
                .help("Add fitting-slot overlays")
                .action(ArgAction::SetTrue),
            Arg::new("clone_overlays")
                .long("clone_overlays")
                .help("Add clone restriction overlays (CUSTOM)")
                .action(ArgAction::SetTrue),
            Arg::new("no_purge")
                .long("no_purge")
                .help("Do not purge icon cache folder")
                .action(ArgAction::SetTrue),
        ])
        .subcommand_required(true)
        .subcommands([
            Command::new("service_bundle")
                .about("Image Service hosting bundle (zip incl. metadata)")
                .arg(
                    Arg::new("out")
                        .short('o')
                        .long("out")
                        .required(true)
                        .help("Output file")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf())
                ),
            Command::new("iec")
                .about("Image Export Collection (zip)")
                .arg(
                    Arg::new("out")
                        .short('o')
                        .long("out")
                        .required(true)
                        .help("Output file")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf())
                ),
            Command::new("web_dir")
                .about("Prepare a directory for web hosting")
                .args([
                    Arg::new("out")
                        .short('o')
                        .long("out")
                        .required(true)
                        .help("Output directory")
                        .value_name("OUTPUT DIRECTORY")
                        .value_parser(ValueParser::path_buf()),
                    Arg::new("copy_files")
                        .long("copy_files")
                        .help("Copy image files rather than creating symlinks")
                        .conflicts_with("hardlink")
                        .action(ArgAction::SetTrue),
                    Arg::new("hardlink")
                        .long("hardlink")
                        .help("Use hard-links rather than soft-links")
                        .conflicts_with("copy_files")
                        .action(ArgAction::SetTrue)
                ]),
            Command::new("checksum")
                .about("Prints (or writes) the checksum of the current icon set")
                .arg(
                    Arg::new("out")
                        .short('o')
                        .long("out")
                        .help("Output file, if omitted, prints checksum to stdout")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf())
                ),
            Command::new("aux_shiptree")
                .about("Auxiliary Ship Tree Render dump (zip)")
                .arg(
                    Arg::new("out")
                        .short('o')
                        .long("out")
                        .required(true)
                        .help("Output file")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf())
                ),
            Command::new("aux_icons")
                .about("Auxiliary Icon dump (zip)")
                .arg(
                    Arg::new("out")
                        .short('o')
                        .long("out")
                        .required(true)
                        .help("Output file")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf())
                ),
            Command::new("aux_all")
                .about("Auxiliary All-Images dump (zip)")
                .args([
                    Arg::new("out")
                        .short('o')
                        .long("out")
                        .required(true)
                        .help("Output file")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf()),
                    Arg::new("incl_character")
                        .help("Include character textures (~5GB of additional data)")
                        .action(ArgAction::SetTrue),
                ]),
            Command::new("multi")
                .about("Perform multiple commands in a single run, see other commands for their details. At least one output option must be specified.")
                .args([
                    Arg::new("service_bundle")
                        .long("service_bundle")
                        .help("Output service bundle")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf()),
                    Arg::new("iec")
                        .long("iec")
                        .help("Output Image Export Collection")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf()),
                    Arg::new("web_dir")
                        .long("web_dir")
                        .help("Prepare a directory for web hosting")
                        .value_name("OUTPUT DIRECTORY")
                        .value_parser(ValueParser::path_buf()),
                    Arg::new("copy_files")
                        .long("copy_files")
                        .help("(web_dir) Copy image files rather than creating symlinks")
                        .conflicts_with("hardlink")
                        .requires("web_dir")
                        .action(ArgAction::SetTrue),
                    Arg::new("hardlink")
                        .long("hardlink")
                        .help("(web_dir) Use hard-links rather than soft-links")
                        .conflicts_with("copy_files")
                        .requires("web_dir")
                        .action(ArgAction::SetTrue),
                    Arg::new("checksum_file")
                        .long("checksum_file")
                        .help("Write checksum to file")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf())
                        .conflicts_with("checksum_stdout"),
                    Arg::new("checksum_stdout")
                        .long("checksum_stout")
                        .help("Write checksum to stdout. Suppresses other stdout output")
                        .conflicts_with("checksum_file"),
                    Arg::new("aux_shiptree")
                        .long("aux_shiptree")
                        .help("Output Auxiliary Ship Tree Render dump")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf()),
                    Arg::new("aux_icons")
                        .long("aux_icons")
                        .help("Output Auxiliary Icon dump")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf()),
                    Arg::new("aux_all")
                        .long("aux_all")
                        .help("Output Auxiliary All-Images dump")
                        .value_name("FILE")
                        .value_parser(ValueParser::path_buf()),
                    Arg::new("incl_character")
                        .long("incl_character")
                        .help("(aux_all) Include character textures (~5GB of additional data)")
                        .requires("aux_all")
                        .action(ArgAction::SetTrue),
                ])
                .arg_required_else_help(true)
        ])
        .get_matches();

    let (command_name, command_args) = arg_matches.subcommand().expect("subcommand required");
    let output_mode = match command_name {
        "service_bundle" => vec![OutputMode::ServiceBundle { out: &command_args.get_one::<PathBuf>("out").expect("out is required") }],
        "iec" => vec![OutputMode::IEC { out: &command_args.get_one::<PathBuf>("out").expect("out is required") }],
        "web_dir" => {
            let out = &command_args.get_one::<PathBuf>("out").expect("out is required");
            if !fs::exists(out)? {
                fs::create_dir_all(out)?;
            } else if fs::metadata(out)?.is_file() {
                Err(std::io::Error::other(format!("Output must be a directory! ({})", out.to_string_lossy())))?;
            }
            vec![OutputMode::Web {
                out,
                copy_files: command_args.get_flag("copy_files"),
                hard_link: command_args.get_flag("hardlink")
            }]
        },
        "checksum" => { vec![OutputMode::Checksum { out: command_args.get_one::<PathBuf>("out").map(PathBuf::as_path) }] },
        "aux_shiptree" => vec![OutputMode::AuxShipTreeRenders { out: &command_args.get_one::<PathBuf>("out").expect("out is required") }],
        "aux_icon" => vec![OutputMode::AuxIcons { out: &command_args.get_one::<PathBuf>("out").expect("out is required") }],
        "aux_all" => {
            vec![OutputMode::AuxImages {
                out: &command_args.get_one::<PathBuf>("out").expect("out is required"),
                incl_character: command_args.get_flag("incl_character")
            }]
        },
        "multi" => {
            let mut output_modes = Vec::with_capacity(6);

            if let Some(out) = command_args.get_one::<PathBuf>("service_bundle") {
                output_modes.push(OutputMode::ServiceBundle { out })
            }

            if let Some(out) = command_args.get_one::<PathBuf>("iec") {
                output_modes.push(OutputMode::IEC { out })
            }

            if let Some(out) = command_args.get_one::<PathBuf>("web_dir") {
                output_modes.push(OutputMode::Web {
                    out,
                    copy_files: command_args.get_flag("copy_files"),
                    hard_link: command_args.get_flag("hardlink")
                })
            }

            if let Some(out) = command_args.get_one::<PathBuf>("aux_icons") {
                output_modes.push(OutputMode::AuxIcons { out })
            }

            if let Some(out) = command_args.get_one::<PathBuf>("aux_shiptree") {
                output_modes.push(OutputMode::AuxShipTreeRenders { out })
            }

            if let Some(out) = command_args.get_one::<PathBuf>("aux_all") {
                output_modes.push(OutputMode::AuxImages { out, incl_character: command_args.get_flag("incl_character") })
            }

            // Do checksum last
            if let Some(out) = command_args.get_one::<PathBuf>("checksum_file") {
                assert!(!command_args.contains_id("checksum_stdout"), "checksum_file conflicts with checksum_stdout");
                output_modes.push(OutputMode::Checksum { out: Some(out) })
            }
            if command_args.contains_id("checksum_stdout") {
                assert!(!command_args.contains_id("checksum_file"), "checksum_stdout conflicts with checksum_file");
                output_modes.push(OutputMode::Checksum { out: None })
            }


            assert!(output_modes.len() > 0, "At least one output mode when using `multi` command is required"); // This should be enforced by clap, so a simple panic is sufficient.
            output_modes
        }
        _ => panic!("Unknown subcommand: {}", command_name)
    };

    if let Some(log_path) = arg_matches.get_one::<PathBuf>("logfile") {
        let mut opts = fs::File::options();
        if arg_matches.get_flag("append_log") {
            opts.create(true).append(true);
        } else {
            opts.create(true).write(true).truncate(true);
        }

        LOG_FILE.set(opts.open(log_path)?).expect("Log file is set only once!");
    }
    let log_file = LOG_FILE.get();
    if let Some(mut log) = log_file { writeln!(log, "Icon generation run - {}", chrono::Local::now())?; }

    let mut user_agent = match (arg_matches.get_one::<String>("user_agent"), arg_matches.get_one::<PathBuf>("user_agent_file")) {
        (Some(_), Some(_)) => unreachable!("Only one UA option may be set"),
        (Some(ua), None) => ua.clone(),
        (None, Some(ua_file)) => fs::read_to_string(ua_file).map_err(|err| IconError::String(format!("could not read User Agent file: {}", err)))?,
        (None, None) => unreachable!("At least one UA option must be set"),
    };

    use std::fmt::Write;    // Write into string
    write!(&mut user_agent, " turtletools:{}/{} +{}", CRATE_NAME, CRATE_VERSION, CRATE_REPO).expect("write into string should not fail!");

    let silent_mode = arg_matches.get_flag("silent"); // icons::build_icon_export overrides this to `true` if "checksum to stdout" is present
    let skip_if_fresh = arg_matches.get_flag("skip_if_fresh");
    let no_purge = arg_matches.get_flag("no_purge");

    let icon_config = IconConfig {
        use_old_overlays: arg_matches.get_flag("old_overlays"),
        module_overlays: arg_matches.get_flag("module_overlays"),
        clone_overlays: arg_matches.get_flag("clone_overlays"),
    };

    let start = Instant::now();
    if !silent_mode { println!("Initializing cache (UA:`{}`)", user_agent); }
    if let Some(mut log) = log_file { writeln!(log, "Initializing cache (UA:`{}`)", user_agent)?; }
    let cache = CacheDownloader::initialize(
        arg_matches.get_one::<PathBuf>("cache_folder").expect("cache_folder is a required argument"),
        false,
        &*user_agent
    )?;
    let cache_init_duration = start.elapsed();

    let data_load_start = Instant::now();
    if !silent_mode { println!("Loading SDE..."); }
    if let Some(mut log) = log_file { writeln!(log, "Loading SDE...")?; }
    let icon_build_data = IconBuildData::load(SDELoader::open_latest("./cache/sde.zip")?, icon_config)?;
    let data_load_duration = data_load_start.elapsed();

    if !silent_mode { println!("Building icons..."); }
    if let Some(mut log) = log_file { writeln!(log, "Building icons...")?; }

    let build_start = Instant::now();
    icons::build_icon_export(
        icon_config,
        output_mode,
        skip_if_fresh,
        no_purge,
        &icon_build_data,
        &cache,
        arg_matches.get_one::<PathBuf>("icon_folder").expect("icon_folder is a required argument"),
        arg_matches.get_flag("force_rebuild"),
        silent_mode
    )?;

    let build_duration = build_start.elapsed();

    let s1 = format!("Finished in: {:.1} seconds", start.elapsed().as_secs_f64());
    let s2 = format!("\tCache init: {:.1} seconds", cache_init_duration.as_secs_f64());
    let s3 = format!("\tData load: {:.1} seconds", data_load_duration.as_secs_f64());
    let s4 = format!("\tImage Build: {:.1} seconds", build_duration.as_secs_f64());

    if !silent_mode {
        println!("{}", s1);
        println!("{}", s2);
        println!("{}", s3);
        println!("{}", s4);
    }
    if let Some(mut log) = log_file {
        writeln!(log, "{}", s1)?;
        writeln!(log, "{}", s2)?;
        writeln!(log, "{}", s3)?;
        writeln!(log, "{}", s4)?;
    }

    // Delete unnecessary cache files to avoid a storage "leak"
    cache.purge(&["sde.zip", "checksum.txt"])?;

    Ok(())
}
