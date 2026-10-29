//! What the ports are read from: `/proc` on Linux. On any other system nothing is found, so the server builds and runs
//! there and reports no ports.

use std::path::PathBuf;

/// A TCP port something listens on, reachable on the server's loopback address.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Socket {
    pub port: u16,
    pub inode: u64,
}

// The readers of `/proc` are used only on Linux; elsewhere they stay, tested, for the tables they read.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// The listening sockets of a `/proc/net/tcp` or `/proc/net/tcp6` table that the loopback address reaches: those bound
/// to every address or to a loopback one. A socket bound to one other address cannot be reached through the tunnel.
pub(super) fn listening(table: &str) -> Vec<Socket> {
    const LISTEN: &str = "0A";
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let local = fields.nth(1)?;
            let state = fields.nth(1)?;
            let inode = fields.nth(5)?.parse().ok()?;
            let (address, port) = local.split_once(':')?;
            (state == LISTEN && reachable(address)).then_some(Socket {
                port: u16::from_str_radix(port, 16).ok()?,
                inode,
            })
        })
        .filter(|socket| socket.inode != 0)
        .collect()
}

// The readers of `/proc` are used only on Linux; elsewhere they stay, tested, for the tables they read.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// Whether an address of the tables, in the kernel's byte order, is every address or a loopback one.
fn reachable(address: &str) -> bool {
    match address.len() {
        // 127.x.x.x: the first octet is the last byte, written last.
        8 => address == "00000000" || address.ends_with("7F"),
        32 => {
            address.bytes().all(|b| b == b'0')
                || address == "00000000000000000000000001000000"
                || (address.starts_with("0000000000000000FFFF0000") && address.ends_with("7F"))
        }
        _ => false,
    }
}

// The readers of `/proc` are used only on Linux; elsewhere they stay, tested, for the tables they read.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
/// The parent of the process `stat` is the `/proc/<pid>/stat` of: the second field after the name, which may itself
/// hold spaces and parentheses, so it is read after the last closing one.
pub(super) fn parent_in(stat: &str) -> Option<u32> {
    let after = &stat[stat.rfind(')')? + 1..];
    after.split_whitespace().nth(1)?.parse().ok()
}

#[cfg(target_os = "linux")]
mod system {
    use std::collections::HashMap;
    use std::path::PathBuf;

    use super::{Socket, listening, parent_in};

    /// Every listening socket reachable through the loopback address, with a process that holds it.
    pub fn listeners() -> Vec<(Socket, u32)> {
        let mut sockets = Vec::new();
        for table in ["/proc/net/tcp", "/proc/net/tcp6"] {
            if let Ok(text) = std::fs::read_to_string(table) {
                sockets.extend(listening(&text));
            }
        }
        if sockets.is_empty() {
            return Vec::new();
        }
        let mut holders: HashMap<u64, u32> = HashMap::new();
        let Ok(processes) = std::fs::read_dir("/proc") else {
            return Vec::new();
        };
        for process in processes.flatten() {
            let Some(pid) = process
                .file_name()
                .to_str()
                .and_then(|n| n.parse::<u32>().ok())
            else {
                continue;
            };
            let Ok(fds) = std::fs::read_dir(process.path().join("fd")) else {
                continue;
            };
            for fd in fds.flatten() {
                let Ok(target) = std::fs::read_link(fd.path()) else {
                    continue;
                };
                let target = target.to_string_lossy();
                if let Some(inode) = target
                    .strip_prefix("socket:[")
                    .and_then(|rest| rest.strip_suffix(']'))
                    .and_then(|inode| inode.parse::<u64>().ok())
                {
                    holders.entry(inode).or_insert(pid);
                }
            }
        }
        sockets
            .into_iter()
            .filter_map(|socket| holders.get(&socket.inode).map(|pid| (socket, *pid)))
            .collect()
    }

    pub fn parent(pid: u32) -> Option<u32> {
        parent_in(&std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?)
    }

    pub fn directory(pid: u32) -> Option<PathBuf> {
        std::fs::read_link(format!("/proc/{pid}/cwd")).ok()
    }

    pub fn name(pid: u32) -> Option<String> {
        let comm = std::fs::read_to_string(format!("/proc/{pid}/comm")).ok()?;
        Some(comm.trim().to_string())
    }
}

#[cfg(not(target_os = "linux"))]
mod system {
    use std::path::PathBuf;

    use super::Socket;

    pub fn listeners() -> Vec<(Socket, u32)> {
        Vec::new()
    }

    pub fn parent(_pid: u32) -> Option<u32> {
        None
    }

    pub fn directory(_pid: u32) -> Option<PathBuf> {
        None
    }

    pub fn name(_pid: u32) -> Option<String> {
        None
    }
}

/// Every TCP port something listens on that the loopback address reaches, with a process that holds its socket.
pub(super) fn listeners() -> Vec<(u16, u32)> {
    system::listeners()
        .into_iter()
        .map(|(socket, pid)| (socket.port, pid))
        .collect()
}

/// The parent of the process `pid`, if it runs and has one.
pub(super) fn parent(pid: u32) -> Option<u32> {
    system::parent(pid).filter(|parent| *parent != 0)
}

/// The working directory of the process `pid`.
pub(super) fn directory(pid: u32) -> Option<PathBuf> {
    system::directory(pid)
}

/// The name of the program the process `pid` runs.
pub(super) fn name(pid: u32) -> Option<String> {
    system::name(pid)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TABLE: &str = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode
   0: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 1111 1 0000 100 0 0 10 0
   1: 00000000:0050 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 2222 1 0000 100 0 0 10 0
   2: 0100007F:1F91 0100007F:C350 01 00000000:00000000 00:00000000 00000000  1000        0 3333 1 0000 100 0 0 10 0
   3: 0A0AA8C0:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 4444 1 0000 100 0 0 10 0
";

    #[test]
    fn listening_sockets_on_loopback_or_every_address_are_read() {
        let found = listening(TABLE);
        assert_eq!(
            found,
            vec![
                Socket {
                    port: 8080,
                    inode: 1111
                },
                Socket {
                    port: 80,
                    inode: 2222
                },
            ],
            "an established connection and a socket on one other address are left out"
        );
    }

    #[test]
    fn the_tables_of_ipv6_are_read_the_same_way() {
        let table = "header
   0: 00000000000000000000000000000000:1F40 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 5555 1 0 100 0 0 10 0
   1: 00000000000000000000000001000000:1F41 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 6666 1 0 100 0 0 10 0
   2: 0000000000000000FFFF00000100007F:1F42 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 7777 1 0 100 0 0 10 0
   3: 20010DB8000000000000000000000001:1F43 00000000000000000000000000000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 8888 1 0 100 0 0 10 0
";
        let ports: Vec<u16> = listening(table).iter().map(|s| s.port).collect();
        assert_eq!(ports, vec![8000, 8001, 8002]);
    }

    #[test]
    fn the_parent_is_read_past_a_name_with_spaces_and_parentheses() {
        assert_eq!(parent_in("42 (my (odd) name) S 7 42 42 0 -1"), Some(7));
        assert_eq!(parent_in("garbage"), None);
    }
}
