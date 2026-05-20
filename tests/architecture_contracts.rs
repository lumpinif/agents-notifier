use std::fs;
use std::path::{Path, PathBuf};

#[test]
fn production_source_paths_do_not_construct_final_signals_directly() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    collect_rust_files(&root.join("src/sources"), &mut files);
    files.push(root.join("src/local_ingress.rs"));

    // This is a lightweight architecture guard, not a compiler-enforced
    // boundary. It catches ordinary direct Signal construction in source paths;
    // if this boundary grows more critical, replace it with stronger module
    // visibility or a real lint.
    let forbidden = [
        "Signal::new(",
        "Signal::new_with_timestamp(",
        "Signal::new_structured_with_timestamp(",
    ];
    for file in files {
        let content = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("failed to read `{}`: {error}", file.display()));
        for pattern in forbidden {
            assert!(
                !content.contains(pattern),
                "`{}` constructs final Signal directly with `{}`; source paths must go through SignalDraft and SignalBuilder",
                file.strip_prefix(&root).unwrap_or(&file).display(),
                pattern
            );
        }
    }
}

fn collect_rust_files(path: &Path, files: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(path)
        .unwrap_or_else(|error| panic!("failed to read `{}`: {error}", path.display()));
    for entry in entries {
        let entry = entry.unwrap_or_else(|error| {
            panic!("failed to read entry in `{}`: {error}", path.display())
        });
        let path = entry.path();
        if path.is_dir() {
            collect_rust_files(&path, files);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
}
