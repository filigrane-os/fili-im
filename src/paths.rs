//! Where Conversations keeps things. Everything derives from the XDG variables (absolute values only) or `HOME`, so a
//! test sets `HOME` and `XDG_RUNTIME_DIR` and gets a private world.
use std::path::{Path, PathBuf};

fn home() -> PathBuf {
    std::env::var_os("HOME").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/"))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).filter(|p| p.is_absolute()).unwrap_or_else(|| home().join(fallback))
}

/// `~/.local/share/fili-im`: the database, media and the accounts' private directories.
pub fn data_dir() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share").join("fili-im")
}

pub fn database() -> PathBuf {
    data_dir().join("conversations.db")
}

/// The private directory of one account (the connector's session and keys).
pub fn account_dir(key: &str) -> PathBuf {
    data_dir().join("accounts").join(key)
}

pub fn media_dir(key: &str) -> PathBuf {
    data_dir().join("media").join(key)
}

/// `~/.local/state/fili-im`: connector logs.
pub fn state_dir() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("fili-im")
}

pub fn connector_log(key: &str) -> PathBuf {
    state_dir().join(format!("{key}.log"))
}

/// `~/.local/state/alerts`: the files the fili-bar's alert modules read (same place as Facteur's).
pub fn alerts_dir() -> PathBuf {
    home().join(".local/state/alerts")
}

/// `$XDG_RUNTIME_DIR/fili-im`: the daemon's socket and lock.
pub fn runtime_dir() -> PathBuf {
    std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| std::env::temp_dir().join(format!("fili-im-{}", unsafe { libc::getuid() })))
        .join("fili-im")
}

pub fn socket() -> PathBuf {
    runtime_dir().join("daemon.sock")
}

/// Create a directory (and its parents) readable by the user only.
pub fn private_dir(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
}

/// Connector programs, in search order: next to `fili-im`, `FILI_IM_CONNECTOR_DIR`, then `PATH`.
pub fn connector_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        dirs.push(dir);
    }
    if let Some(dir) = std::env::var_os("FILI_IM_CONNECTOR_DIR") {
        dirs.push(PathBuf::from(dir));
    }
    if let Some(path) = std::env::var_os("PATH") {
        dirs.extend(std::env::split_paths(&path));
    }
    dirs
}

const PREFIX: &str = "fili-im-";

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    path.metadata().is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// A platform name is lower-case ASCII letters and digits: it becomes part of a program name and of account keys.
pub fn valid_platform(name: &str) -> bool {
    !name.is_empty() && name.len() <= 32 && name.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

pub fn find_connector(platform: &str) -> Option<PathBuf> {
    if !valid_platform(platform) {
        return None;
    }
    connector_dirs().into_iter().map(|dir| dir.join(format!("{PREFIX}{platform}"))).find(|p| is_executable(p))
}

/// Installed connectors: (platform, program), first found wins.
pub fn connectors() -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = Vec::new();
    for dir in connector_dirs() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        let mut names: Vec<_> = entries.flatten().collect();
        names.sort_by_key(|e| e.file_name());
        for entry in names {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(platform) = name.strip_prefix(PREFIX) else { continue };
            if valid_platform(platform) && is_executable(&entry.path()) && !found.iter().any(|(p, _)| p == platform) {
                found.push((platform.to_string(), entry.path()));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_names_are_plain() {
        assert!(valid_platform("whatsapp"));
        assert!(valid_platform("matrix2"));
        assert!(!valid_platform(""));
        assert!(!valid_platform("../x"));
        assert!(!valid_platform("WhatsApp"));
        assert!(!valid_platform("a-b"));
    }
}
