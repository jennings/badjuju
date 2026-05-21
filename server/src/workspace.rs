use std::path::{Path, PathBuf};

/// Walk up from `start` to find the nearest ancestor directory containing `.jj/`.
pub fn find_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = start;
    loop {
        if current.join(".jj").is_dir() {
            return Some(current.to_path_buf());
        }
        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    fn init_jj_repo(dir: &Path) {
        Command::new("jj")
            .args(["git", "init"])
            .current_dir(dir)
            .output()
            .expect("jj git init failed");
    }

    #[test]
    fn finds_root_from_repo_root() {
        let dir = tempdir().unwrap();
        init_jj_repo(dir.path());
        let found = find_workspace_root(dir.path());
        assert_eq!(found, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn finds_root_from_subdirectory() {
        let dir = tempdir().unwrap();
        init_jj_repo(dir.path());
        let subdir = dir.path().join("src").join("deep");
        std::fs::create_dir_all(&subdir).unwrap();
        let found = find_workspace_root(&subdir);
        assert_eq!(found, Some(dir.path().to_path_buf()));
    }

    #[test]
    fn returns_none_outside_any_repo() {
        let dir = tempdir().unwrap();
        let found = find_workspace_root(dir.path());
        assert!(found.is_none());
    }
}
