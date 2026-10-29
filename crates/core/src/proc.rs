//! Subprocess helpers for adb and fastboot.

use crate::report::Reporter;
use crate::Result;
use std::process::{Command, Output};

pub fn run(rep: &dyn Reporter, prog: &str, args: &[&str]) -> Result<Output> {
    rep.log(&format!("$ {} {}", prog, args.join(" ")));
    Command::new(prog)
        .args(args)
        .output()
        .map_err(|e| anyhow::anyhow!("could not run {prog}: {e}"))
}

pub fn run_ok(rep: &dyn Reporter, prog: &str, args: &[&str]) -> Result<Output> {
    let out = run(rep, prog, args)?;
    if !out.status.success() {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        anyhow::bail!(
            "{} {} failed ({})\n{}{}",
            prog,
            args.join(" "),
            out.status,
            stdout,
            stderr
        );
    }
    Ok(out)
}

pub fn run_stdout(rep: &dyn Reporter, prog: &str, args: &[&str]) -> Result<String> {
    let out = run_ok(rep, prog, args)?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

pub fn adb(rep: &dyn Reporter, args: &[&str]) -> Result<String> {
    run_stdout(rep, "adb", args)
}

pub fn adb_ok(rep: &dyn Reporter, args: &[&str]) -> Result<()> {
    run_ok(rep, "adb", args).map(|_| ())
}

pub fn fastboot(rep: &dyn Reporter, args: &[&str]) -> Result<String> {
    run_stdout(rep, "fastboot", args)
}

pub fn fastboot_ok(rep: &dyn Reporter, args: &[&str]) -> Result<()> {
    run_ok(rep, "fastboot", args).map(|_| ())
}
