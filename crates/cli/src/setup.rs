//! `aria-memo setup` — establish and persist the CLI config (mirrors the
//! trimmed "cli config" portion of aria-router's `setup`, without the gateway
//! recipe / keys / users which memo does not have).
//!
//! The only persisted setting today is `upgrade_url`, consumed by
//! `aria-memo upgrade`. It is an org root (e.g. `https://github.com/ariacompute`)
//! — `upgrade` appends `/memo/releases` to build the API path.
//!
//! Usage:
//!   aria-memo setup            # choose upgrade source (GitHub/Gitee), persist it
//!   aria-memo setup --status   # print config status
//!   aria-memo setup --clear    # delete config file

use crate::config::{
    clear_cli_config, default_cli_config_path, default_upgrade_url_for_site, load_cli_config,
    save_cli_config, MemoCliConfig, DEFAULT_UPGRADE_URL_CN, DEFAULT_UPGRADE_URL_COM,
};
use clap::Args;
use std::io::{self, BufRead, Write};

#[derive(Args)]
pub struct SetupArgs {
    /// Show config status
    #[arg(long)]
    pub status: bool,
    /// Remove the CLI config file
    #[arg(long)]
    pub clear: bool,
}

/// Run `aria-memo setup`.
pub fn run(args: SetupArgs) -> io::Result<()> {
    if args.status {
        return setup_status();
    }
    if args.clear {
        return setup_clear();
    }
    setup_write()
}

/// Persist `upgrade_url` by selecting the Releases org root (GitHub or Gitee).
///
/// Interactive (TTY): present a choice (default GitHub, or Gitee when
/// `ARIA_MEMO_SITE=cn`). Non-interactive: honor `ARIA_MEMO_SITE`, then keep an
/// existing config, then fall back to the GitHub default.
fn setup_write() -> io::Result<()> {
    let existing = load_cli_config()?;
    let site = std::env::var("ARIA_MEMO_SITE").unwrap_or_default();
    let default_url = default_upgrade_url_for_site(&site).to_string();

    let upgrade_url = if stdin_is_tty() {
        prompt_choice(&default_url)?
    } else if !site.trim().is_empty() {
        default_url
    } else if !existing.upgrade_url.is_empty() {
        existing.upgrade_url.clone()
    } else {
        DEFAULT_UPGRADE_URL_COM.to_string()
    };

    let path = save_cli_config(&MemoCliConfig {
        upgrade_url: upgrade_url.clone(),
    })?;
    println!("wrote {} (upgrade_url={upgrade_url})", path.display());
    Ok(())
}

/// Prompt for the upgrade source: 1) GitHub, 2) Gitee.
fn prompt_choice(default_url: &str) -> io::Result<String> {
    println!("Select the upgrade source for `aria-memo upgrade`:");
    println!("  1) GitHub  {DEFAULT_UPGRADE_URL_COM}");
    println!("  2) Gitee   {DEFAULT_UPGRADE_URL_CN}");
    let default_choice: u8 = if default_url.contains("gitee.com") { 2 } else { 1 };
    let ans = prompt(&format!("Choice [{default_choice}]: "))?;
    let choice = ans.trim();
    if choice.is_empty() {
        Ok(default_url.to_string())
    } else if choice == "1" {
        Ok(DEFAULT_UPGRADE_URL_COM.to_string())
    } else if choice == "2" {
        Ok(DEFAULT_UPGRADE_URL_CN.to_string())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid choice: {choice} (expected 1 or 2)"),
        ))
    }
}

fn setup_status() -> io::Result<()> {
    let path = default_cli_config_path()?;
    println!("config: {}", path.display());
    if path.exists() {
        let cli = load_cli_config()?;
        if cli.upgrade_url.is_empty() {
            println!("upgrade_url: (not set)");
        } else {
            println!("upgrade_url: {}", cli.upgrade_url);
        }
    } else {
        println!("(missing; run aria-memo setup)");
    }
    Ok(())
}

fn setup_clear() -> io::Result<()> {
    if let Some(p) = clear_cli_config()? {
        println!("cleared {}", p.display());
    } else {
        println!("nothing to clear");
    }
    Ok(())
}

fn prompt(label: &str) -> io::Result<String> {
    eprint!("{label}");
    io::stderr().flush()?;
    let mut line = String::new();
    io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_string())
}

fn stdin_is_tty() -> bool {
    use std::io::IsTerminal;
    std::io::stdin().is_terminal()
}
