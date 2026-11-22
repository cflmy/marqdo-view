//! Architectural gates (RFC-VIEW-0001 §17.2): no private Core crates, no
//! source-tree reads of the legacy repositories, no second renderer, no shell
//! execution, one CSS/JS entry point.

use std::path::{Path, PathBuf};

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
}

fn collect_files(dir: &Path, extension: &str, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, extension, out);
        } else if path.extension().and_then(|e| e.to_str()) == Some(extension) {
            out.push(path);
        }
    }
}

#[test]
fn no_private_core_crate_dependencies() {
    let root = manifest_dir();
    let forbidden = [
        "marqdo",
        "marqdo-cli",
        "marqdo-ir",
        "marqdo-runtime",
        "marqdo-frontend",
        "marqdo-host",
        "marqdo-markdown",
        "marqdo-span",
        "marqdo-expression",
        "marqdo-sema",
    ];
    let mut in_dependency_section = false;
    for line in read(&root.join("Cargo.toml")).lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_dependency_section = trimmed.ends_with("dependencies]")
                || trimmed.contains("dependencies.")
                || trimmed.contains("dependencies]");
            continue;
        }
        if !in_dependency_section || trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((name, _)) = trimmed.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_matches('"');
        assert!(
            !forbidden.contains(&name),
            "Cargo.toml declares forbidden Core dependency {name:?}"
        );
    }

    let lock = read(&root.join("Cargo.lock"));
    for line in lock.lines() {
        let Some(rest) = line.trim().strip_prefix("name = \"") else {
            continue;
        };
        let name = rest.trim_end_matches('"');
        if name == "marqdo-view" {
            continue;
        }
        assert!(
            !name.starts_with("marqdo") || name == "marqdo",
            "Cargo.lock contains suspicious package {name:?}"
        );
        assert!(
            !forbidden.contains(&name),
            "Cargo.lock contains forbidden Core package {name:?}"
        );
    }
}

#[test]
fn source_never_reads_or_links_the_legacy_repositories() {
    let root = manifest_dir();
    let mut sources = Vec::new();
    collect_files(&root.join("src"), "rs", &mut sources);
    assert!(!sources.is_empty(), "src/ has no Rust sources");
    let needles = [
        ["marqdo", "_ir"].concat(),
        ["marqdo", "-ir"].concat(),
        ["marqdo", "_runtime"].concat(),
        ["marqdo", "-runtime"].concat(),
        ["marqdo", "_frontend"].concat(),
        ["marqdo", "-frontend"].concat(),
        ["marqdo", "::"].concat(),
        ["..", "/marqdo"].concat(),
        ["..", "/marqdo-cli"].concat(),
        ["inspect", " --ir"].concat(),
        ["Command::new", "(\"sh\")"].concat(),
        ["Command::new", "(\"bash\")"].concat(),
        ["sh", " -c"].concat(),
        ["bash", " -c"].concat(),
    ];
    for path in sources {
        let text = read(&path);
        for (line_number, line) in text.lines().enumerate() {
            let trimmed = line.trim_start();
            // Documentation may *name* forbidden APIs when explaining what is
            // not used; the gate only scans executable source lines.
            if trimmed.starts_with("//") {
                continue;
            }
            for needle in &needles {
                assert!(
                    !line.contains(needle.as_str()),
                    "{}:{} contains forbidden reference {:?}",
                    path.display(),
                    line_number + 1,
                    needle
                );
            }
        }
    }
}

#[test]
fn exactly_one_reader_theme_and_script_entry() {
    let root = manifest_dir();
    let assets = root.join("assets");
    let mut css = Vec::new();
    let mut js = Vec::new();
    collect_files(&assets, "css", &mut css);
    collect_files(&assets, "js", &mut js);
    assert!(css.len() <= 1, "more than one CSS entry: {css:?}");
    assert!(js.len() <= 1, "more than one JS entry: {js:?}");
    for path in &css {
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("reader.css"),
            "the only CSS entry must be assets/reader.css"
        );
    }
    for path in &js {
        assert_eq!(
            path.file_name().and_then(|n| n.to_str()),
            Some("reader.js"),
            "the only JS entry must be assets/reader.js"
        );
    }
    // The source tree must not reference legacy skin names at all.
    let mut sources = Vec::new();
    collect_files(&root.join("src"), "rs", &mut sources);
    for path in sources {
        let text = read(&path);
        for needle in ["notebook-page", "reader-page", "legacy.css", "new.css"] {
            assert!(
                !text.contains(needle),
                "{} references legacy skin marker {needle:?}",
                path.display()
            );
        }
    }
}

#[test]
fn tool_manifest_matches_package_and_protocols() {
    let root = manifest_dir();
    let raw = read(&root.join("tool.json"));
    let tool: serde_json::Value = serde_json::from_str(&raw).expect("tool.json is JSON");
    assert_eq!(tool["schema"], "marqdo.cli-tool");
    assert_eq!(tool["schema_version"], 1);
    assert_eq!(tool["name"], "marqdo-view");
    assert_eq!(tool["command"], "view");
    assert_eq!(tool["entrypoint"], "bin/marqdo-view");
    assert_eq!(
        tool["version"],
        env!("CARGO_PKG_VERSION"),
        "tool.json version must match Cargo package version"
    );
    assert_eq!(tool["compatibility"]["marqdo"], ">=3.0.0-beta.1,<4.0.0");
    assert_eq!(tool["compatibility"]["analysis"], serde_json::json!([2]));
    assert_eq!(tool["compatibility"]["trace"], serde_json::json!([2]));
}

#[test]
fn release_lockfile_is_committed() {
    let root = manifest_dir();
    assert!(
        root.join("Cargo.lock").is_file(),
        "Cargo.lock must be committed for --locked release builds"
    );
}
