//! Starts the game the way the original launcher did, and checks whether it or the server is running.

use crate::model::ServerAddress;
use std::{
    fs::OpenOptions,
    io,
    net::{TcpStream, ToSocketAddrs},
    path::Path,
    process::Command,
    time::Duration,
};

pub const GAME_EXE: &str = "gameclient_release.exe";

/// All five are required and must not be empty, or the client refuses to start.
pub fn game_args(a: &ServerAddress) -> Vec<String> {
    vec![
        format!("-ip:{}", a.ip),
        format!("-port:{}", a.port),
        format!("-zone:{}", a.zone),
        format!("-gametype:{}", a.gametype),
        format!("-nettype:{}", a.nettype),
    ]
}

/// The Advanced setting `address:port` replaces the official address and port; zone and types stay.
pub fn with_override(base: &ServerAddress, over: Option<&str>) -> Result<ServerAddress, String> {
    let Some(over) = over.map(str::trim).filter(|o| !o.is_empty()) else {
        return Ok(base.clone());
    };
    let (ip, port) = over
        .rsplit_once(':')
        .ok_or("Use the form address:port, for example 192.168.1.20:7000.")?;
    let port: u16 = port
        .parse()
        .map_err(|_| "The port must be a number from 1 to 65535.")?;
    if ip.trim().is_empty() || port == 0 {
        return Err("Use the form address:port, for example 192.168.1.20:7000.".into());
    }
    Ok(ServerAddress {
        ip: ip.trim().to_string(),
        port,
        ..base.clone()
    })
}

pub fn start_game(folder: &Path, a: &ServerAddress) -> io::Result<()> {
    Command::new(folder.join(GAME_EXE))
        .args(game_args(a))
        .current_dir(folder)
        .spawn()
        .map(|_| ())
}

/// True while the game exe in this folder is running: Windows refuses to open a running exe for writing
/// (sharing violation, error 32).
pub fn exe_in_use(folder: &Path) -> bool {
    match OpenOptions::new().write(true).open(folder.join(GAME_EXE)) {
        Ok(_) => false,
        Err(e) => e.raw_os_error() == Some(32),
    }
}

pub fn server_up(host: &str, port: u16, timeout: Duration) -> bool {
    (host, port)
        .to_socket_addrs()
        .map(|addrs| {
            addrs
                .into_iter()
                .any(|addr| TcpStream::connect_timeout(&addr, timeout).is_ok())
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn official() -> ServerAddress {
        ServerAddress {
            ip: "203.0.113.5".into(),
            port: 7000,
            zone: 1,
            gametype: 1,
            nettype: 0,
        }
    }

    #[test]
    fn builds_the_five_required_arguments() {
        assert_eq!(
            game_args(&official()),
            vec![
                "-ip:203.0.113.5",
                "-port:7000",
                "-zone:1",
                "-gametype:1",
                "-nettype:0"
            ]
        );
    }

    #[test]
    fn an_override_replaces_only_address_and_port() {
        assert_eq!(with_override(&official(), None).unwrap(), official());
        assert_eq!(with_override(&official(), Some("  ")).unwrap(), official());
        let o = with_override(&official(), Some("127.0.0.1:7100")).unwrap();
        assert_eq!((o.ip.as_str(), o.port, o.zone), ("127.0.0.1", 7100, 1));
    }

    #[test]
    fn a_bad_override_is_explained() {
        for bad in ["127.0.0.1", "127.0.0.1:abc", ":7000", "host:0"] {
            assert!(with_override(&official(), Some(bad)).is_err(), "{bad}");
        }
    }

    #[test]
    fn server_up_sees_a_listening_port_and_not_a_closed_one() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        assert!(server_up("127.0.0.1", port, Duration::from_secs(2)));
        drop(listener);
        assert!(!server_up("127.0.0.1", port, Duration::from_millis(500)));
    }

    #[cfg(windows)]
    #[test]
    fn a_running_game_exe_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        let ping = std::path::PathBuf::from(std::env::var("SystemRoot").unwrap())
            .join(r"System32\PING.EXE");
        std::fs::copy(ping, dir.path().join(GAME_EXE)).unwrap();
        assert!(!exe_in_use(dir.path()));
        let mut child = Command::new(dir.path().join(GAME_EXE))
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_millis(500));
        let busy = exe_in_use(dir.path());
        child.kill().unwrap();
        child.wait().unwrap();
        std::thread::sleep(Duration::from_millis(300));
        assert!(busy, "a running exe is in use");
        assert!(!exe_in_use(dir.path()), "a stopped exe is free");
    }
}
