use clap::{Parser, Subcommand};
use std::io::{self, Write};
use std::path::PathBuf;
use t405a_core::{self as core, Settings, StdoutReporter};

#[derive(Parser)]
#[command(
    name = "t405a-gsi",
    version,
    about = "Unlock, back up and install LineageOS (or another GSI) on the onn 11\" Core (T405A)"
)]
struct Cli {
    /// Images, logs and backups (default: the platform data directory)
    #[arg(long, global = true)]
    workdir: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Show device, tools and bootloader compatibility
    Check,
    /// Pre-fetch and verify the GSI images
    Download {
        #[arg(value_parser = ["trebledroid", "lineage", "lineage-gapps", "all"])]
        what: String,
    },
    /// Unlock the bootloader (wipes userdata)
    Unlock {
        #[arg(long)]
        yes: bool,
    },
    /// Dump stock partitions via a DSU userdebug GSI
    Backup {
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Flash a raw ext4 GSI (LineageOS by default)
    Flash {
        #[arg(long)]
        backup: PathBuf,
        /// Use your own raw ext4 arm64 GSI instead of the bundled LineageOS image
        #[arg(long)]
        image: Option<PathBuf>,
        /// Use the LineageOS GSI built with Google apps (GApps)
        #[arg(long, conflicts_with = "image")]
        gapps: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// Restore stock boot/vbmeta/super from a backup
    Restore {
        #[arg(long)]
        backup: PathBuf,
        #[arg(long)]
        no_super: bool,
        #[arg(long)]
        dry_run: bool,
    },
    /// check -> unlock -> backup -> flash
    All {
        #[arg(long)]
        yes: bool,
    },
}

fn default_workdir() -> PathBuf {
    dirs::data_dir()
        .or_else(dirs::home_dir)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("t405a-gsi")
}

fn confirm_unlock() -> bool {
    print!("Type exactly: UNLOCK T405A\n> ");
    let _ = io::stdout().flush();
    let mut line = String::new();
    if io::stdin().read_line(&mut line).is_err() {
        return false;
    }
    line.trim() == "UNLOCK T405A"
}

fn run() -> anyhow::Result<()> {
    let cli = Cli::parse();
    let settings = Settings::new(cli.workdir.unwrap_or_else(default_workdir));
    let rep = StdoutReporter;

    match cli.cmd {
        Cmd::Check => {
            core::check(&rep)?;
        }
        Cmd::Download { what } => match what.as_str() {
            "trebledroid" => {
                core::download::ensure_trebledroid(&rep, &settings)?;
            }
            "lineage" => {
                core::download::ensure_lineage(&rep, &settings, false)?;
            }
            "lineage-gapps" => {
                core::download::ensure_lineage(&rep, &settings, true)?;
            }
            _ => core::download_all(&rep, &settings)?,
        },
        Cmd::Unlock { yes } => {
            if !yes && !confirm_unlock() {
                anyhow::bail!("confirmation did not match; nothing done");
            }
            core::unlock::unlock(&rep, &settings)?;
        }
        Cmd::Backup { out } => {
            core::backup::backup(&rep, &settings, out.as_deref())?;
        }
        Cmd::Flash {
            backup,
            image,
            gapps,
            dry_run,
        } => {
            core::flash::flash(&rep, &settings, &backup, image.as_deref(), gapps, dry_run)?;
        }
        Cmd::Restore {
            backup,
            no_super,
            dry_run,
        } => {
            core::flash::restore(&rep, &settings, &backup, no_super, dry_run)?;
        }
        Cmd::All { yes } => {
            core::check(&rep)?;
            let summary = core::device::require_model(&rep)?;
            if summary.locked != "0" {
                if !yes && !confirm_unlock() {
                    anyhow::bail!("confirmation did not match; nothing done");
                }
                core::unlock::unlock(&rep, &settings)?;
            }
            let backup = core::backup::backup(&rep, &settings, None)?;
            core::flash::flash(&rep, &settings, &backup, None, false, false)?;
        }
    }
    Ok(())
}

fn main() {
    if let Err(err) = run() {
        eprintln!("ERROR: {err:#}");
        std::process::exit(1);
    }
}
