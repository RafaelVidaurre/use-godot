mod support;

use tempfile::TempDir;

use support::ug;

#[test]
fn conflicting_arguments_fail_during_cli_parsing() {
    let root = TempDir::new().unwrap();
    ug(root.path())
        .args(["default", "4.7", "--unset"])
        .assert()
        .failure();
    ug(root.path())
        .args(["list", "--prerelease"])
        .assert()
        .failure();
    ug(root.path())
        .args(["install", "4.7", "--checksum", "abc"])
        .assert()
        .failure();
}
