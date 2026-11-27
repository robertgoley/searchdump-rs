//! Finding the searchdump binary and probing which flags it accepts.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Stdio;

/// 1.1-7: whether `bin` is searchdump (which takes `--statsFile`), not Node elasticdump (which
/// would get an option it does not know). The `elasticdump`/`elasticdump-rs` compatibility
/// names are symlinks to searchdump, so the name is checked after resolving links.
pub(crate) fn takes_stats_file(bin: &Path) -> bool {
    let real = fs::canonicalize(bin).unwrap_or_else(|_| bin.to_path_buf());
    real.file_name()
        .and_then(|n| n.to_str())
        .is_some_and(|n| n.starts_with("searchdump"))
}

/// 1.1-7: whether this `bin` build accepts `--statsFile` (searchdump 1.1+). A searchdump 1.0.x
/// given with `--elasticdump-bin` has the name but not the option, and would fail every data
/// run on it. clap rejects an unknown option before it acts on `--help`, so this exits 0 only
/// when the option is known. Run once at startup.
pub(crate) fn accepts_server_limits(bin: &Path) -> bool {
    std::process::Command::new(bin)
        .arg("--serverLimits=off")
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub(crate) fn accepts_stats_file(bin: &Path) -> bool {
    std::process::Command::new(bin)
        .arg("--statsFile=/dev/null")
        .arg("--help")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

pub(crate) fn find_bin(explicit: Option<&Path>) -> PathBuf {
    if let Some(p) = explicit {
        return p.to_path_buf();
    }
    if let Some(p) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("searchdump")))
        .filter(|p| p.is_file())
    {
        return p;
    }
    if let Some(p) = searchdump_rs::envnames::var("SEARCHDUMP_BIN") {
        return PathBuf::from(p);
    }
    PathBuf::from("searchdump")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_file_only_for_searchdump() {
        assert!(takes_stats_file(Path::new("searchdump")));
        assert!(takes_stats_file(Path::new("/opt/x/searchdump")));
        assert!(!takes_stats_file(Path::new(
            "/usr/lib/node_modules/.bin/elasticdump"
        )));
        #[cfg(unix)]
        {
            let d = tempfile::tempdir().unwrap();
            let real = d.path().join("searchdump");
            fs::write(&real, "").unwrap();
            let link = d.path().join("elasticdump");
            std::os::unix::fs::symlink(&real, &link).unwrap();
            assert!(
                takes_stats_file(&link),
                "compatibility symlink to searchdump"
            );
        }
    }
}
