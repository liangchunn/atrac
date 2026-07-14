use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(root: impl AsRef<Path>) -> Vec<PathBuf> {
    fn visit(path: &Path, files: &mut Vec<PathBuf>) {
        for entry in fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                visit(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }

    let mut files = Vec::new();
    if root.as_ref().is_file() {
        files.push(root.as_ref().to_owned());
    } else {
        visit(root.as_ref(), &mut files);
    }
    files
}

fn assert_code_excludes(root: impl AsRef<Path>, forbidden: &[&str]) {
    for path in rust_files(root) {
        let source = fs::read_to_string(&path).unwrap();
        for (line_index, line) in source.lines().enumerate() {
            let line = line.trim_start();
            if line.starts_with("//") {
                continue;
            }
            for pattern in forbidden {
                assert!(
                    !line.contains(pattern),
                    "{}:{} crosses an architecture boundary with `{pattern}`",
                    path.display(),
                    line_index + 1,
                );
            }
        }
    }
}

#[test]
fn layer_dependencies_remain_directed() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    assert_code_excludes(source.join("core/coding"), &["crate::core::bitstream"]);
    assert_code_excludes(source.join("core/bitstream.rs"), &["crate::encoder"]);
    assert_code_excludes(
        source.join("tables"),
        &["crate::analysis", "crate::core", "crate::encoder"],
    );
}
