//! Platform-specific process inspection helpers.
//!
//! Used to discover the foreground process of a PTY so the UI can display
//! meaningful tab titles and status information. Falls back gracefully on
//! platforms or situations where detection is unavailable.

use std::path::PathBuf;
use std::process::Command;

/// Information about the process currently in the foreground of a PTY.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ForegroundProcess {
    /// Process id of the foreground process (or its group leader).
    pub pid: u32,
    /// Human-readable process name.
    pub name: String,
    /// Process group id that owns the foreground.
    pub pgid: i32,
}

/// Discover the foreground process of the terminal whose master fd is
/// `fd`. Returns `None` when detection is unavailable or no foreground
/// process exists.
#[cfg(unix)]
pub fn foreground_process(fd: std::os::unix::io::RawFd) -> Option<ForegroundProcess> {
    // tcgetpgrp returns the foreground process group of the terminal.
    // The master side of a pty refers to the same terminal, so this works.
    let pgid = unsafe { libc::tcgetpgrp(fd) };
    if pgid <= 0 {
        return None;
    }

    // Find a member of that process group. Prefer the group leader
    // (pid == pgid); otherwise take any member.
    let (pid, comm) = find_process_in_group(pgid)?;

    Some(ForegroundProcess {
        pid,
        name: comm,
        pgid,
    })
}

#[cfg(not(unix))]
pub fn foreground_process(_fd: i32) -> Option<ForegroundProcess> {
    None
}

/// Snapshot of all processes grouped by process-group id.
///
/// Runs a single `ps` invocation and returns `pgid -> [(pid, comm), ...]`.
/// Used by the foreground-process poller so each tick costs one `ps` call
/// regardless of session count.
#[cfg(unix)]
pub fn process_groups() -> std::collections::HashMap<i32, Vec<(u32, String)>> {
    use std::collections::HashMap;
    let mut map: HashMap<i32, Vec<(u32, String)>> = HashMap::new();
    let output = match Command::new("ps")
        .args(["-axo", "pid=,pgid=,comm="])
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return map,
    };
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        let mut parts = line.split_whitespace();
        let pid: u32 = match parts.next().and_then(|s| s.parse().ok()) {
            Some(p) => p,
            None => continue,
        };
        let pgid: i32 = match parts.next().and_then(|s| s.parse().ok()) {
            Some(g) => g,
            None => continue,
        };
        let comm = parts.collect::<Vec<_>>().join(" ");
        if comm.is_empty() {
            continue;
        }
        map.entry(pgid).or_default().push((pid, base_name(&comm)));
    }
    map
}

/// Look up a process in process group `pgid` using `ps`.
#[cfg(unix)]
fn find_process_in_group(pgid: i32) -> Option<(u32, String)> {
    let groups = process_groups();
    let members = groups.get(&pgid)?;
    members
        .iter()
        .find(|(p, _)| *p as i32 == pgid)
        .or_else(|| members.first())
        .cloned()
}

/// Reduce a command path/name to a short display name.
fn base_name(comm: &str) -> String {
    let trimmed = comm.trim();
    if let Some(idx) = trimmed.rfind('/') {
        trimmed[idx + 1..].to_string()
    } else {
        trimmed.to_string()
    }
}

/// Resolve the current working directory of a process, if possible.
///
/// On Linux this reads `/proc/<pid>/cwd`; elsewhere we fall back to
/// `lsof`. Returns `None` on failure (e.g. permissions, dead process).
#[cfg(target_os = "linux")]
pub fn process_cwd(pid: u32) -> Option<PathBuf> {
    std::fs::read_link(format!("/proc/{}/cwd", pid)).ok()
}

#[cfg(all(unix, not(target_os = "linux")))]
pub fn process_cwd(pid: u32) -> Option<PathBuf> {
    let output = Command::new("lsof")
        .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    for line in text.lines() {
        if let Some(path) = line.strip_prefix('n') {
            let p = PathBuf::from(path);
            if p.is_absolute() {
                return Some(p);
            }
        }
    }
    None
}

#[cfg(not(unix))]
pub fn process_cwd(_pid: u32) -> Option<PathBuf> {
    None
}

/// Parse an OSC 7 `file://` URI into a local path.
///
/// Format: `file://hostname/path` — the hostname is ignored for local
/// sessions (it is typically `localhost` or empty).
pub fn parse_osc7_cwd(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    // Skip authority component (up to first '/') if present and non-empty.
    let path_start = if rest.starts_with('/') {
        // file:///path — authority empty
        0
    } else {
        match rest.find('/') {
            Some(i) => i,
            None => return None,
        }
    };
    let path = &rest[path_start..];
    if path.is_empty() {
        return None;
    }
    // Percent-decode minimal set used by terminals.
    let decoded = percent_decode(path);
    let p = PathBuf::from(decoded);
    if p.is_absolute() {
        Some(p)
    } else {
        None
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(v);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn osc7_parse() {
        assert_eq!(
            parse_osc7_cwd("file://localhost/home/user"),
            Some(PathBuf::from("/home/user"))
        );
        assert_eq!(
            parse_osc7_cwd("file:///tmp/x%20y"),
            Some(PathBuf::from("/tmp/x y"))
        );
        assert_eq!(parse_osc7_cwd("not a uri"), None);
    }
}
