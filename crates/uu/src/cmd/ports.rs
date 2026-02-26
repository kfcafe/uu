//! `uu ports` — list and kill processes by port.

use std::process::{Command, Stdio};

use anyhow::{bail, Context, Result};

use crate::runner::style;

/// A process listening on a TCP port.
#[derive(Debug)]
struct Listener {
    command: String,
    pid: u32,
    user: String,
    port: u16,
}

pub(crate) fn execute(port: Option<u16>, kill: bool) -> Result<()> {
    let listeners = list_listeners()?;

    if kill {
        let port =
            port.ok_or_else(|| anyhow::anyhow!("specify a port to kill: uu ports -k 3000"))?;
        return kill_port(&listeners, port);
    }

    let filtered: Vec<&Listener> = match port {
        Some(p) => listeners.iter().filter(|l| l.port == p).collect(),
        None => listeners.iter().collect(),
    };

    if filtered.is_empty() {
        match port {
            Some(p) => eprintln!("{} nothing listening on :{p}", style("32", "clear")),
            None => eprintln!("{} no listening TCP ports found", style("32", "clear")),
        }
        return Ok(());
    }

    // Header
    eprintln!("  {:>6}  {:>7}  {:<16} USER", "PORT", "PID", "COMMAND");

    for l in &filtered {
        eprintln!(
            "  {:>6}  {:>7}  {:<16} {}",
            l.port, l.pid, l.command, l.user
        );
    }

    eprintln!(
        "\n  {} listener{}",
        filtered.len(),
        if filtered.len() == 1 { "" } else { "s" }
    );

    Ok(())
}

fn kill_port(listeners: &[Listener], port: u16) -> Result<()> {
    let on_port: Vec<&Listener> = listeners.iter().filter(|l| l.port == port).collect();

    if on_port.is_empty() {
        bail!("nothing listening on :{port}");
    }

    for l in &on_port {
        eprintln!(
            "{} {} \x1b[2m(pid {}, :{})\x1b[0m",
            style("31", "killing"),
            l.command,
            l.pid,
            l.port
        );

        let status = Command::new("kill")
            .arg(l.pid.to_string())
            .status()
            .context("failed to send kill signal")?;

        if !status.success() {
            eprintln!(
                "{} could not kill pid {} — try: sudo kill -9 {}",
                style("33", "warning"),
                l.pid,
                l.pid
            );
        }
    }

    Ok(())
}

/// Parse `lsof` output to find listening TCP sockets.
fn list_listeners() -> Result<Vec<Listener>> {
    let output = Command::new("lsof")
        .args(["-iTCP", "-sTCP:LISTEN", "-nP"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .context("failed to run lsof — is it installed?")?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut listeners = Vec::new();

    for line in stdout.lines().skip(1) {
        if let Some(l) = parse_lsof_line(line) {
            listeners.push(l);
        }
    }

    // Deduplicate by (port, pid) — lsof may list IPv4 + IPv6 separately
    listeners.sort_by_key(|l| (l.port, l.pid));
    listeners.dedup_by_key(|l| (l.port, l.pid));

    Ok(listeners)
}

/// Parse a single lsof output line into a Listener.
///
/// lsof columns: COMMAND PID USER FD TYPE DEVICE SIZE/OFF NODE NAME
/// NAME looks like `*:3000` or `127.0.0.1:8080`, possibly followed by `(LISTEN)`.
fn parse_lsof_line(line: &str) -> Option<Listener> {
    let fields: Vec<&str> = line.split_whitespace().collect();
    if fields.len() < 9 {
        return None;
    }

    let command = fields[0].to_owned();
    let pid: u32 = fields[1].parse().ok()?;
    let user = fields[2].to_owned();

    // The address:port field — either last or second-to-last (before "(LISTEN)")
    let name_field = if fields.last() == Some(&"(LISTEN)") {
        fields.get(fields.len().wrapping_sub(2))?
    } else {
        fields.last()?
    };

    // Extract port from after the last ':'
    let port: u16 = name_field.rsplit(':').next()?.parse().ok()?;

    Some(Listener {
        command,
        pid,
        user,
        port,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ipv4_listen_line() {
        let line = "node      12345 asher   23u  IPv4 0xabc  0t0  TCP *:3000 (LISTEN)";
        let l = parse_lsof_line(line).unwrap();
        assert_eq!(l.command, "node");
        assert_eq!(l.pid, 12345);
        assert_eq!(l.user, "asher");
        assert_eq!(l.port, 3000);
    }

    #[test]
    fn parse_ipv6_listen_line() {
        let line = "node      12345 asher   24u  IPv6 0xdef  0t0  TCP [::1]:8080 (LISTEN)";
        let l = parse_lsof_line(line).unwrap();
        assert_eq!(l.port, 8080);
    }

    #[test]
    fn parse_localhost_binding() {
        let line = "postgres  999   asher   5u  IPv4 0x123  0t0  TCP 127.0.0.1:5432 (LISTEN)";
        let l = parse_lsof_line(line).unwrap();
        assert_eq!(l.command, "postgres");
        assert_eq!(l.port, 5432);
    }

    #[test]
    fn parse_short_line_returns_none() {
        assert!(parse_lsof_line("too short").is_none());
    }

    #[test]
    fn parse_non_numeric_pid_returns_none() {
        let line = "node  BADPID asher  23u  IPv4 0xabc  0t0  TCP *:3000 (LISTEN)";
        assert!(parse_lsof_line(line).is_none());
    }
}
