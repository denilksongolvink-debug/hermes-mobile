use std::fs;

/// Read RSS in KB for a single process via /proc/{pid}/status (VmRSS).
pub fn read_rss_kb(pid: u32) -> Option<u64> {
    let path = format!("/proc/{}/status", pid);
    let content = fs::read_to_string(&path).ok()?;
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("VmRSS:") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if parts.len() >= 2 && parts[1] == "kB" {
                return parts[0].parse::<u64>().ok();
            }
            // sometimes unit missing? try first token
            if let Some(first) = parts.first() {
                if let Ok(v) = first.parse::<u64>() {
                    return Some(v);
                }
            }
        }
    }
    None
}

/// List child PIDs of a process from /proc/{pid}/task/{pid}/children.
pub fn child_pids(pid: u32) -> Vec<u32> {
    let path = format!("/proc/{}/task/{}/children", pid, pid);
    let content = match fs::read_to_string(&path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    content
        .split_whitespace()
        .filter_map(|s| s.parse::<u32>().ok())
        .collect()
}

/// Sum RSS of process and all descendants.
pub fn read_tree_rss_kb(pid: u32) -> u64 {
    let mut total = read_rss_kb(pid).unwrap_or(0);
    for c in child_pids(pid) {
        total += read_tree_rss_kb(c);
    }
    total
}

/// Whether process is alive.
pub fn is_alive(pid: u32) -> bool {
    // /proc/{pid} exists
    std::path::Path::new(&format!("/proc/{}", pid)).exists()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_process_rss_positive() {
        let pid = std::process::id();
        let rss = read_rss_kb(pid).unwrap();
        assert!(rss > 0, "rss should be >0, got {}", rss);
    }

    #[test]
    fn tree_rss_at_least_self() {
        let pid = std::process::id();
        let self_rss = read_rss_kb(pid).unwrap();
        let tree = read_tree_rss_kb(pid);
        assert!(tree >= self_rss);
    }

    #[test]
    fn is_alive_self() {
        assert!(is_alive(std::process::id()));
    }

    #[test]
    fn is_alive_bogus() {
        assert!(!is_alive(999_999_999));
    }
}
