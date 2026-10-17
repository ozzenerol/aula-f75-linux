//! Installs the udev rule with one polkit password prompt, so the app works without sudo.
use anyhow::{bail, Context, Result};
use std::process::Command;

pub const RULE: &str = include_str!("../packaging/70-aula-f75.rules");
const DEST: &str = "/etc/udev/rules.d/70-aula-f75.rules";

pub fn rule_installed() -> bool {
    std::fs::read_to_string(DEST).is_ok_and(|s| s == RULE)
}

/// pkexec copies the rule, removes the old 99- rule, reloads udev and re-applies it to
/// existing hidraw nodes (cable, receiver and Bluetooth) - no replug needed.
pub fn fix() -> Result<()> {
    let tmp = std::env::temp_dir().join(format!("f75-studio-{}.rules", std::process::id()));
    std::fs::write(&tmp, RULE).context("write temp rule")?;
    let script = format!(
        "install -m 0644 '{tmp}' {DEST} && rm -f /etc/udev/rules.d/99-aula-f75.rules && \
         udevadm control --reload-rules && udevadm trigger --subsystem-match=hidraw --action=change && udevadm settle",
        tmp = tmp.display()
    );
    let status = Command::new("pkexec").args(["sh", "-c", &script]).status();
    let _ = std::fs::remove_file(&tmp);
    match status {
        Ok(s) if s.success() => Ok(()),
        Ok(s) if s.code() == Some(126) || s.code() == Some(127) => bail!("password prompt was cancelled"),
        Ok(s) => bail!("permission setup failed (exit {s})"),
        Err(e) => bail!("pkexec not available ({e}) - run ./install.sh instead"),
    }
}
