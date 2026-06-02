use std::path::Path;

#[derive(serde::Deserialize)]
pub struct SquashSelection {
    pub file: String,
    pub hunk_header: String,
    pub hunk_content: String,
    pub direction: String,
}

/// Parse `@@ -(old_start)[,(old_len)] ... @@` and return `(old_start, old_len)`.
/// Both are returned as 0-based (we subtract 1 from the 1-indexed start).
pub fn parse_hunk_old_range(header: &str) -> Option<(usize, usize)> {
    let after = header.trim_start_matches(|c: char| c == '@' || c == ' ');
    let after = after.strip_prefix('-')?;
    let end = after.find(|c: char| c == ' ' || c == '+')?;
    let range = &after[..end];
    if let Some(comma) = range.find(',') {
        let start: usize = range[..comma].parse().ok()?;
        let len: usize = range[comma + 1..].parse().ok()?;
        Some((start, len))
    } else {
        let start: usize = range.parse().ok()?;
        Some((start, 1))
    }
}

/// Apply `hunk_content` (diff lines with `+`/`-`/` ` prefixes) to `left_content`,
/// starting at the position described by `hunk_header`. Returns the resulting
/// file content.
pub fn apply_hunk(left_content: &str, hunk_header: &str, hunk_content: &str) -> String {
    let (old_start, old_len) = parse_hunk_old_range(hunk_header).unwrap_or((1, 0));

    let left_lines: Vec<&str> = left_content.lines().collect();
    let mut out: Vec<&str> = Vec::new();

    // Lines before old_start (1-indexed → 0-indexed prefix = 0..old_start-1)
    let before = old_start.saturating_sub(1).min(left_lines.len());
    out.extend_from_slice(&left_lines[..before]);

    // Process hunk content
    for line in hunk_content.lines() {
        if let Some(rest) = line.strip_prefix('+') {
            out.push(rest);
        } else if let Some(rest) = line.strip_prefix(' ') {
            out.push(rest);
        }
        // '-' lines: skip (deleted from left)
    }

    // Lines after old_start + old_len
    let after = (old_start.saturating_sub(1) + old_len).min(left_lines.len());
    out.extend_from_slice(&left_lines[after..]);

    let mut result = out.join("\n");
    if left_content.ends_with('\n') && !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

/// Recursively collect all file paths (relative to `base`) under `dir`.
fn walk_files(dir: &Path, base: &Path) -> std::io::Result<Vec<std::path::PathBuf>> {
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Ok(files);
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(walk_files(&path, base)?);
        } else if let Ok(rel) = path.strip_prefix(base) {
            files.push(rel.to_path_buf());
        }
    }
    Ok(files)
}

/// Execute the squash-tool merge-tool operation: read `sidecar_path` for the
/// hunk selection, then mutate `right_dir` so that only the selected hunk
/// (or all-except-it, for `direction=exclude`) survives.
pub fn run(
    left_dir: &Path,
    right_dir: &Path,
    sidecar_path: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let sidecar_str = std::fs::read_to_string(sidecar_path)?;
    let selection: SquashSelection = serde_json::from_str(&sidecar_str)?;

    match selection.direction.as_str() {
        "include" => {
            let target = &selection.file;

            // Apply only the selected hunk to the target file.
            let left_file = left_dir.join(target);
            let right_file = right_dir.join(target);

            if left_file.exists() {
                let left_content = std::fs::read_to_string(&left_file)?;
                let new_content = apply_hunk(
                    &left_content,
                    &selection.hunk_header,
                    &selection.hunk_content,
                );
                if let Some(parent) = right_file.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::write(&right_file, new_content)?;
            }

            // Restore all OTHER files in right_dir to left_dir state.
            let right_files = walk_files(right_dir, right_dir).unwrap_or_default();
            for rel in &right_files {
                if rel.to_string_lossy() == target.as_str() {
                    continue;
                }
                let left_src = left_dir.join(rel);
                let right_dst = right_dir.join(rel);
                if left_src.exists() {
                    std::fs::copy(&left_src, &right_dst)?;
                } else {
                    // File was added in the source; remove it from right.
                    let _ = std::fs::remove_file(&right_dst);
                }
            }

            // Also restore files that were deleted (present in left but absent in right).
            let left_files = walk_files(left_dir, left_dir).unwrap_or_default();
            for rel in &left_files {
                if rel.to_string_lossy() == target.as_str() {
                    continue;
                }
                let right_dst = right_dir.join(rel);
                if !right_dst.exists() {
                    if let Some(parent) = right_dst.parent() {
                        std::fs::create_dir_all(parent)?;
                    }
                    std::fs::copy(left_dir.join(rel), &right_dst)?;
                }
            }
        }
        other => return Err(format!("unknown direction: {other}").into()),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_hunk_old_range_simple() {
        assert_eq!(parse_hunk_old_range("@@ -1,4 +1,5 @@"), Some((1, 4)));
    }

    #[test]
    fn parse_hunk_old_range_no_len() {
        assert_eq!(parse_hunk_old_range("@@ -3 +3 @@"), Some((3, 1)));
    }

    #[test]
    fn parse_hunk_old_range_zero_start() {
        // "@@ -0,0 +1,3 @@" means new file
        assert_eq!(parse_hunk_old_range("@@ -0,0 +1,3 @@"), Some((0, 0)));
    }

    #[test]
    fn apply_hunk_replaces_lines() {
        let left = "line1\nline2\nline3\nline4\n";
        let header = "@@ -2,2 +2,2 @@";
        let content = " line2\n-line3\n+LINE3\n";
        let result = apply_hunk(left, header, content);
        assert_eq!(result, "line1\nline2\nLINE3\nline4\n");
    }

    #[test]
    fn apply_hunk_add_only() {
        let left = "a\nb\n";
        let header = "@@ -1,0 +1,1 @@"; // Insert after line 1 (old_start=1, old_len=0)
        // With old_start=1, old_len=0: prefix=0..0=empty, suffix=0.., so inserts at start
        // Actually: before=0, after=0, so prefix=[], content=[+new], suffix=[a,b]
        // This inserts "new" before everything
        let content = "+new\n";
        let result = apply_hunk(left, header, content);
        // before=0 lines (old_start.saturating_sub(1)=0), content=["new"], after=0..
        assert_eq!(result, "new\na\nb\n");
    }

    #[test]
    fn apply_hunk_removes_lines() {
        let left = "a\nb\nc\n";
        let header = "@@ -2,1 +1,0 @@";
        let content = "-b\n";
        let result = apply_hunk(left, header, content);
        assert_eq!(result, "a\nc\n");
    }

    #[test]
    fn run_include_applies_only_selected_hunk() {
        let dir = tempfile::tempdir().unwrap();
        let left = dir.path().join("left");
        let right = dir.path().join("right");
        std::fs::create_dir_all(&left).unwrap();
        std::fs::create_dir_all(&right).unwrap();

        // left/file.txt has 4 lines
        std::fs::write(left.join("file.txt"), "a\nb\nc\nd\n").unwrap();
        // right/file.txt has hunk 1 and hunk 2 applied
        std::fs::write(right.join("file.txt"), "A\nb\nC\nd\n").unwrap();

        // Another file changed in right
        std::fs::write(left.join("other.txt"), "x\n").unwrap();
        std::fs::write(right.join("other.txt"), "X\n").unwrap();

        let sidecar = dir.path().join("sel.json");
        std::fs::write(
            &sidecar,
            r#"{"file":"file.txt","hunk_header":"@@ -1,1 +1,1 @@","hunk_content":"-a\n+A\n","direction":"include"}"#,
        )
        .unwrap();

        run(&left, &right, &sidecar).unwrap();

        // Target file: only hunk1 applied (a→A), hunk2 reverted (c stays c)
        let result = std::fs::read_to_string(right.join("file.txt")).unwrap();
        assert_eq!(
            result, "A\nb\nc\nd\n",
            "target file should have only selected hunk"
        );

        // Other file: restored from left
        let other = std::fs::read_to_string(right.join("other.txt")).unwrap();
        assert_eq!(other, "x\n", "other file should be restored from left");
    }
}
