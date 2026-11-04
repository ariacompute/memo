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
use std::io::{self, BufRead, IsTerminal, Write};

#[derive(Args)]
pub struct SetupArgs {
    /// Show config status
    #[arg(long)]
    pub status: bool,
    /// Remove the CLI config file
    #[arg(long)]
    pub clear: bool,
}

/// Interactive input seam: production reads stdin, tests drive a script.
pub trait Prompter {
    /// Whether the session is interactive (a TTY).
    fn is_tty(&self) -> bool;
    /// Read one line (trimmed).
    fn read_line(&mut self) -> io::Result<String>;
}

/// [`Prompter`] over an arbitrary buffered reader, which makes the prompt flow
/// testable without touching the real terminal. The reader is boxed so the impl
/// is instantiated once (stdin in production, a cursor in tests).
pub struct StreamPrompter {
    pub tty: bool,
    pub input: Box<dyn BufRead>,
}

impl Prompter for StreamPrompter {
    fn is_tty(&self) -> bool {
        self.tty
    }

    fn read_line(&mut self) -> io::Result<String> {
        let mut line = String::new();
        self.input.read_line(&mut line)?;
        Ok(line.trim().to_string())
    }
}

/// Run `aria-memo setup`.
pub fn run(args: SetupArgs) -> io::Result<()> {
    let stdin = io::stdin();
    let tty = stdin.is_terminal();
    let mut prompter = StreamPrompter {
        tty,
        input: Box::new(stdin.lock()),
    };
    run_with(args, &mut prompter)
}

/// Run `aria-memo setup` against an injected prompter.
pub fn run_with(args: SetupArgs, prompter: &mut dyn Prompter) -> io::Result<()> {
    if args.status {
        return setup_status();
    }
    if args.clear {
        return setup_clear();
    }
    setup_write(prompter)
}

/// Persist `upgrade_url` by selecting the Releases org root (GitHub or Gitee).
///
/// Interactive (TTY): present a choice (default GitHub, or Gitee when
/// `ARIA_MEMO_SITE=cn`). Non-interactive: honor `ARIA_MEMO_SITE`, then keep an
/// existing config, then fall back to the GitHub default.
fn setup_write(prompter: &mut dyn Prompter) -> io::Result<()> {
    let existing = load_cli_config()?;
    let site = std::env::var("ARIA_MEMO_SITE").unwrap_or_default();
    let default_url = default_upgrade_url_for_site(&site).to_string();

    let upgrade_url = if prompter.is_tty() {
        prompt_choice(prompter, &default_url)?
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
fn prompt_choice(prompter: &mut dyn Prompter, default_url: &str) -> io::Result<String> {
    println!("Select the upgrade source for `aria-memo upgrade`:");
    println!("  1) GitHub  {DEFAULT_UPGRADE_URL_COM}");
    println!("  2) Gitee   {DEFAULT_UPGRADE_URL_CN}");
    let default_choice: u8 = if default_url.contains("gitee.com") {
        2
    } else {
        1
    };
    let ans = prompt(prompter, &format!("Choice [{default_choice}]: "))?;
    if ans.is_empty() {
        Ok(default_url.to_string())
    } else if ans == "1" {
        Ok(DEFAULT_UPGRADE_URL_COM.to_string())
    } else if ans == "2" {
        Ok(DEFAULT_UPGRADE_URL_CN.to_string())
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid choice: {ans} (expected 1 or 2)"),
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

fn prompt(prompter: &mut dyn Prompter, label: &str) -> io::Result<String> {
    eprint!("{label}");
    io::stderr().flush()?;
    prompter.read_line()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env_lock, temp_dir};
    use std::io::Cursor;

    /// Prompter over a scripted input; `tty` mirrors an interactive session.
    fn script(tty: bool, input: &str) -> StreamPrompter {
        StreamPrompter {
            tty,
            input: Box::new(Cursor::new(input.as_bytes().to_vec())),
        }
    }

    fn args(status: bool, clear: bool) -> SetupArgs {
        SetupArgs { status, clear }
    }

    #[test]
    fn prompt_choices_cover_github_gitee_blank_and_invalid() {
        assert_eq!(
            prompt_choice(&mut script(true, "1\n"), DEFAULT_UPGRADE_URL_COM).unwrap(),
            DEFAULT_UPGRADE_URL_COM
        );
        assert_eq!(
            prompt_choice(&mut script(true, "2\n"), DEFAULT_UPGRADE_URL_COM).unwrap(),
            DEFAULT_UPGRADE_URL_CN
        );
        // Blank keeps the site default (Gitee default when the URL is Gitee).
        assert_eq!(
            prompt_choice(&mut script(true, "\n"), DEFAULT_UPGRADE_URL_CN).unwrap(),
            DEFAULT_UPGRADE_URL_CN
        );
        assert_eq!(
            prompt_choice(&mut script(true, "  \n"), DEFAULT_UPGRADE_URL_COM).unwrap(),
            DEFAULT_UPGRADE_URL_COM
        );
        // Anything else is rejected (never silently persisted).
        let err = prompt_choice(&mut script(true, "7\n"), DEFAULT_UPGRADE_URL_COM).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn tty_setup_writes_the_selected_source() {
        let _g = env_lock();
        let home = temp_dir("setup-tty");
        std::env::set_var("ARIA_COMPUTE_HOME", &home);
        std::env::remove_var("ARIA_MEMO_SITE");

        run_with(args(false, false), &mut script(true, "2\n")).unwrap();
        assert_eq!(
            load_cli_config().unwrap().upgrade_url,
            DEFAULT_UPGRADE_URL_CN
        );

        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn non_tty_setup_honors_site_then_existing_config_then_default() {
        let _g = env_lock();
        let home = temp_dir("setup-nontty");
        std::env::set_var("ARIA_COMPUTE_HOME", &home);

        // 1) `ARIA_MEMO_SITE=cn` wins when non-interactive.
        std::env::set_var("ARIA_MEMO_SITE", "cn");
        run_with(args(false, false), &mut script(false, "")).unwrap();
        assert_eq!(
            load_cli_config().unwrap().upgrade_url,
            DEFAULT_UPGRADE_URL_CN
        );

        // 2) No site: an existing config is preserved.
        std::env::remove_var("ARIA_MEMO_SITE");
        run_with(args(false, false), &mut script(false, "")).unwrap();
        assert_eq!(
            load_cli_config().unwrap().upgrade_url,
            DEFAULT_UPGRADE_URL_CN
        );

        // 3) No site and no config: GitHub default.
        std::fs::remove_file(default_cli_config_path().unwrap()).unwrap();
        run_with(args(false, false), &mut script(false, "")).unwrap();
        assert_eq!(
            load_cli_config().unwrap().upgrade_url,
            DEFAULT_UPGRADE_URL_COM
        );

        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn status_reports_missing_empty_and_configured() {
        let _g = env_lock();
        let home = temp_dir("setup-status");
        std::env::set_var("ARIA_COMPUTE_HOME", &home);
        std::env::remove_var("ARIA_MEMO_SITE");

        // Missing config file.
        run_with(args(true, false), &mut script(false, "")).unwrap();
        assert!(!default_cli_config_path().unwrap().exists());

        // Config present but `upgrade_url` not set.
        save_cli_config(&MemoCliConfig::default()).unwrap();
        run_with(args(true, false), &mut script(false, "")).unwrap();

        // Config present with a value.
        save_cli_config(&MemoCliConfig {
            upgrade_url: DEFAULT_UPGRADE_URL_CN.into(),
        })
        .unwrap();
        run_with(args(true, false), &mut script(false, "")).unwrap();
        assert_eq!(
            load_cli_config().unwrap().upgrade_url,
            DEFAULT_UPGRADE_URL_CN
        );

        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn clear_reports_removed_and_nothing_to_clear() {
        let _g = env_lock();
        let home = temp_dir("setup-clear");
        std::env::set_var("ARIA_COMPUTE_HOME", &home);

        // Nothing to clear yet.
        run_with(args(false, true), &mut script(false, "")).unwrap();
        // Create then clear.
        save_cli_config(&MemoCliConfig {
            upgrade_url: DEFAULT_UPGRADE_URL_COM.into(),
        })
        .unwrap();
        run_with(args(false, true), &mut script(false, "")).unwrap();
        assert!(!default_cli_config_path().unwrap().exists());

        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn run_entrypoint_uses_the_stdin_prompter_for_status() {
        let _g = env_lock();
        let home = temp_dir("setup-run");
        std::env::set_var("ARIA_COMPUTE_HOME", &home);
        // `--status` never reads stdin, so the real prompter is safe here.
        run(args(true, false)).unwrap();
        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn stream_prompter_reads_trimmed_lines_and_reports_tty() {
        let mut p = script(true, "  hello  \n");
        assert!(p.is_tty());
        assert_eq!(p.read_line().unwrap(), "hello");
        // EOF yields an empty line rather than an error.
        assert_eq!(p.read_line().unwrap(), "");
        assert!(!script(false, "").is_tty());
    }
}
