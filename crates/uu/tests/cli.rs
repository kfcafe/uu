use assert_cmd::cargo::cargo_bin_cmd;
use predicates::prelude::*;
use std::fs;
use tempfile::tempdir;

fn uu() -> assert_cmd::Command {
    cargo_bin_cmd!("uu")
}

// -- Help & version ----------------------------------------------------------

#[test]
fn help_shows_all_subcommands() {
    uu().arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("install"))
        .stdout(predicate::str::contains("run"))
        .stdout(predicate::str::contains("test"))
        .stdout(predicate::str::contains("clean"))
        .stdout(predicate::str::contains("ports"));
}

#[test]
fn version_flag() {
    uu().arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains("uu "));
}

// -- Install dry-run ---------------------------------------------------------

#[test]
fn install_dry_run_cargo() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Rust"))
        .stderr(predicate::str::contains("cargo install --path ."));
}

#[test]
fn install_dry_run_go() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("go.mod"), "module x").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Go"))
        .stderr(predicate::str::contains("go install ./..."));
}

#[test]
fn install_dry_run_node_npm() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Node.js"))
        .stderr(predicate::str::contains("npm install"));
}

#[test]
fn install_dry_run_node_yarn() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();
    fs::write(dir.path().join("yarn.lock"), "").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("yarn install"));
}

#[test]
fn install_dry_run_python() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("pyproject.toml"), "").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("Python"))
        .stderr(predicate::str::contains("install ."));
}

#[test]
fn install_dry_run_cmake() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("CMakeLists.txt"), "").unwrap();

    uu().args(["install", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cmake -B build"))
        .stderr(predicate::str::contains("cmake --install build"));
}

#[test]
fn install_dry_run_with_extra_args() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]\nname = \"x\"").unwrap();

    uu().args(["install", "-n", "--", "--release"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cargo install --path . --release"));
}

#[test]
fn install_no_project_fails() {
    let dir = tempdir().unwrap();

    uu().args(["install"])
        .current_dir(dir.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("no recognized project"))
        .stderr(predicate::str::contains("supported project files"));
}

// -- Run dry-run -------------------------------------------------------------

#[test]
fn run_dry_run_cargo() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "").unwrap();

    uu().args(["run", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cargo run"));
}

#[test]
fn run_dry_run_go() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("go.mod"), "module x").unwrap();

    uu().args(["run", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("go run ."));
}

// -- Test dry-run ------------------------------------------------------------

#[test]
fn test_dry_run_cargo() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "").unwrap();

    uu().args(["test", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("cargo test"));
}

#[test]
fn test_dry_run_go() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("go.mod"), "module x").unwrap();

    uu().args(["test", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("go test ./..."));
}

// -- Clean dry-run -----------------------------------------------------------

#[test]
fn clean_dry_run_node_shows_size() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();

    // Create a fake node_modules with some content
    let nm = dir.path().join("node_modules");
    fs::create_dir(&nm).unwrap();
    fs::write(nm.join("fake.js"), "x".repeat(1024)).unwrap();

    uu().args(["clean", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("node_modules/"))
        .stderr(predicate::str::contains("would"));
}

#[test]
fn clean_dry_run_nothing_to_clean() {
    let dir = tempdir().unwrap();
    fs::write(dir.path().join("package.json"), "{}").unwrap();
    // No node_modules directory exists

    uu().args(["clean", "-n"])
        .current_dir(dir.path())
        .assert()
        .success()
        .stderr(predicate::str::contains("nothing to clean"));
}

// -- Map ---------------------------------------------------------------------

#[test]
fn map_shows_in_help() {
    uu().arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("map"));
}

#[test]
fn map_help() {
    uu().args(["map", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("manifest"))
        .stdout(predicate::str::contains("--output"))
        .stdout(predicate::str::contains("--format"))
        .stdout(predicate::str::contains("--stdout"))
        .stdout(predicate::str::contains("--detect-only"))
        .stdout(predicate::str::contains("--diff"))
        .stdout(predicate::str::contains("--dry-run"))
        .stdout(predicate::str::contains("--all"));
}

#[test]
fn map_detect_only_rust() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

    uu().args(["map", "--detect-only", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("Rust"))
        .stderr(predicate::str::contains("rust"));
}

#[test]
fn map_rust_project() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub struct MyWidget {\n    pub name: String,\n}\n",
    )
    .unwrap();

    uu().args(["map", "--stdout", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("project:"))
        .stdout(predicate::str::contains("kind: Rust"));
}

#[test]
fn map_node_project() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("package.json"),
        r#"{"name": "test-app", "version": "1.0.0"}"#,
    )
    .unwrap();
    fs::write(dir.path().join("index.js"), "module.exports = {}").unwrap();

    uu().args(["map", "--stdout", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("project:"))
        .stdout(predicate::str::contains("Node"));
}

#[test]
fn map_stdout_flag() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

    // --stdout should output YAML to stdout and NOT create a file
    uu().args(["map", "--stdout", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("project:"));

    assert!(!dir.path().join(".map.yaml").exists());
}

#[test]
fn map_dry_run() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

    uu().args(["map", "-n", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .stderr(predicate::str::contains("Would write"));

    // Must NOT create the manifest file
    assert!(!dir.path().join(".map.yaml").exists());
}

#[test]
fn map_stdout_filters_private_symbols_by_default() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub struct PublicType;\nstruct PrivateType;\n\npub fn public_fn() {}\nfn private_fn() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn only_for_tests() {}\n}\n",
    )
    .unwrap();

    let output = uu()
        .args(["map", "--stdout", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();

    assert!(!output.contains("visibility: Private"));
    assert!(!output.contains("private_fn"));
    assert!(!output.contains("only_for_tests"));
    assert!(output.contains("PublicType"));
    assert!(output.contains("public_fn"));
}

#[test]
fn map_all_includes_private_symbols() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(
        dir.path().join("src/lib.rs"),
        "pub struct PublicType;\nstruct PrivateType;\n\npub fn public_fn() {}\nfn private_fn() {}\n\n#[cfg(test)]\nmod tests {\n    #[test]\n    fn only_for_tests() {}\n}\n",
    )
    .unwrap();

    let output = uu()
        .args(["map", "--all", "--stdout", dir.path().to_str().unwrap()])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let output = String::from_utf8(output).unwrap();

    assert!(output.contains("visibility: Private"));
    assert!(output.contains("private_fn"));
}

#[test]
fn map_format_json() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

    let output = uu()
        .args([
            "map",
            "--stdout",
            "--format",
            "json",
            dir.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();

    // Verify it's valid JSON
    let parsed: serde_json::Value =
        serde_json::from_slice(&output).expect("output should be valid JSON");
    assert!(parsed.get("project").is_some());
}

#[test]
fn map_default_output_file() {
    let dir = tempdir().unwrap();
    fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"x\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::create_dir_all(dir.path().join("src")).unwrap();
    fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();

    uu().args(["map", dir.path().to_str().unwrap()])
        .assert()
        .success();

    // Verify .map.yaml was created
    assert!(dir.path().join(".map.yaml").exists());

    let content = fs::read_to_string(dir.path().join(".map.yaml")).unwrap();
    assert!(content.contains("project:"));
}

// -- Ports -------------------------------------------------------------------

#[test]
fn ports_runs_without_error() {
    // Just verify it doesn't crash — output depends on system state
    uu().args(["ports"]).assert().success();
}
