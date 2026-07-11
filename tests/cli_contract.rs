mod support;

use std::{collections::BTreeSet, fs, path::Path};

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{Map, Value};
use tempfile::{TempDir, tempdir};

use support::{fake_godot, ug};

fn success_json(command: &mut Command) -> Value {
    let output = command.assert().success().get_output().stdout.clone();
    serde_json::from_slice(&output).expect("command emitted valid JSON")
}

fn object(value: &Value) -> &Map<String, Value> {
    value.as_object().expect("value is a JSON object")
}

fn string_field<'a>(value: &'a Value, field: &str) -> &'a str {
    object(value)
        .get(field)
        .unwrap_or_else(|| panic!("missing required JSON field '{field}'"))
        .as_str()
        .unwrap_or_else(|| panic!("JSON field '{field}' is a string"))
}

fn assert_installation_json(value: &Value, expected_variant: &str) {
    let installation = object(value);
    let identity = installation
        .get("identity")
        .expect("installation has identity");
    assert_eq!(string_field(identity, "version"), "4.7.0");
    assert_eq!(string_field(identity, "channel"), "stable");
    assert_eq!(string_field(identity, "variant"), expected_variant);
    assert!(!string_field(identity, "platform").is_empty());
    assert!(!string_field(identity, "arch").is_empty());
    assert!(!string_field(value, "binary").is_empty());
    assert!(
        installation
            .get("installed_at_unix")
            .is_some_and(Value::is_u64)
    );
    assert!(
        installation
            .get("sha256")
            .is_some_and(|digest| digest.is_null() || digest.is_string())
    );

    let source = installation
        .get("source")
        .expect("installation has provenance");
    assert_eq!(string_field(source, "type"), "imported");
    assert!(!string_field(source, "original_path").is_empty());
}

fn installed_manifest(root: &Path) -> Value {
    let installation = fs::read_dir(root.join("versions"))
        .unwrap()
        .find_map(|entry| {
            let entry = entry.unwrap();
            entry.file_type().unwrap().is_dir().then_some(entry.path())
        })
        .expect("one installed version");
    serde_json::from_slice(&fs::read(installation.join("manifest.json")).unwrap()).unwrap()
}

#[test]
fn conflicting_arguments_fail_during_cli_parsing() {
    let root = TempDir::new().unwrap();
    ug(root.path())
        .args(["default", "4.7", "--unset"])
        .assert()
        .code(2);
    ug(root.path())
        .args(["list", "--prerelease"])
        .assert()
        .code(2);
    ug(root.path())
        .args(["install", "4.7", "--checksum", "abc"])
        .assert()
        .code(2);
}

#[test]
fn json_and_persisted_state_contracts_have_stable_required_fields() {
    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    let source = fake_godot(&sources, "Godot-contract");

    let installed = success_json(
        ug(root.path())
            .args(["--json", "install", "4.7@double", "--from"])
            .arg(&source),
    );
    assert_installation_json(&installed, "double");

    let listed = success_json(ug(root.path()).args(["--json", "list"]));
    let listed = listed.as_array().expect("list JSON is an array");
    assert_eq!(listed.len(), 1);
    assert_installation_json(&listed[0], "double");

    let selected = success_json(ug(root.path()).args(["--json", "which", "4.7@double"]));
    assert_installation_json(&selected, "double");

    ug(root.path())
        .args(["--quiet", "alias", "set", "physics", "4.7@double"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    let aliases = success_json(ug(root.path()).args(["--json", "alias", "list"]));
    let canonical = string_field(&aliases, "physics");
    assert!(canonical.starts_with("4.7.0-stable@double+"));

    ug(root.path())
        .args(["--quiet", "default", "physics"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    let default = success_json(ug(root.path()).args(["--json", "default"]));
    assert_eq!(string_field(&default, "default"), canonical);

    let current = success_json(ug(root.path()).args(["--json", "current"]));
    assert_installation_json(&current, "double");

    let pinned = success_json(ug(root.path()).args(["--json", "pin", "physics"]));
    assert_eq!(string_field(&pinned, "selector"), "physics");
    assert!(string_field(&pinned, "path").ends_with(".ugrc"));
    let project_file = root.path().join(".test-environment/cwd/.ugrc");
    assert_eq!(fs::read_to_string(project_file).unwrap(), "physics\n");
    success_json(ug(root.path()).args(["--json", "which"]));

    let state: Value =
        serde_json::from_slice(&fs::read(root.path().join("state.json")).unwrap()).unwrap();
    let state = object(&state);
    assert!(state.get("aliases").is_some_and(Value::is_object));
    assert!(state.get("default").is_some_and(Value::is_string));
    assert!(state.get("active").is_some_and(Value::is_string));

    let manifest = installed_manifest(root.path());
    assert_installation_json(&manifest, "double");
    let binary = string_field(&manifest, "binary");
    assert!(!Path::new(binary).is_absolute());

    let identity_fields: BTreeSet<_> = object(
        object(&manifest)
            .get("identity")
            .expect("manifest identity"),
    )
    .keys()
    .map(String::as_str)
    .collect();
    for required in ["version", "channel", "variant", "platform", "arch"] {
        assert!(identity_fields.contains(required));
    }
}

#[test]
fn quiet_suppresses_routine_output_but_not_json_or_query_results() {
    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    let source = fake_godot(&sources, "Godot-quiet");

    ug(root.path())
        .args(["--quiet", "install", "4.7@double", "--from"])
        .arg(source)
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
    ug(root.path())
        .args(["--quiet", "list"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    ug(root.path())
        .args(["--quiet", "use", "4.7@double"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
    ug(root.path())
        .args(["--quiet", "doctor"])
        .assert()
        .code(0)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());

    let listed = success_json(ug(root.path()).args(["--quiet", "--json", "list"]));
    assert_eq!(listed.as_array().unwrap().len(), 1);
    ug(root.path())
        .args(["--quiet", "which", "4.7@double"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Godot-quiet"));
}

#[test]
fn documented_selector_and_older_state_shapes_remain_compatible() {
    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    let source = fake_godot(&sources, "Godot-beta");
    ug(root.path())
        .args(["--quiet", "install", "4.8-beta2@custom:studio", "--from"])
        .arg(source)
        .assert()
        .success();

    let canonical = fs::read_dir(root.path().join("versions"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .file_name()
        .to_string_lossy()
        .into_owned();
    for selector in [
        "beta",
        "4.8-beta",
        "4.8-beta2",
        "4.8-beta2@custom:studio",
        &canonical,
    ] {
        ug(root.path())
            .args(["which", selector])
            .assert()
            .success()
            .stdout(predicate::str::contains("Godot-beta"));
    }

    let older_state = tempdir().unwrap();
    fs::write(
        older_state.path().join("state.json"),
        br#"{"default":null,"active":null}"#,
    )
    .unwrap();
    let aliases = success_json(ug(older_state.path()).args(["--json", "alias", "list"]));
    assert_eq!(aliases, serde_json::json!({}));
}

#[test]
fn documented_exit_statuses_and_doctor_json_are_stable() {
    let healthy = tempdir().unwrap();
    let checks = success_json(ug(healthy.path()).args(["--json", "doctor"]));
    let checks = checks.as_array().expect("doctor JSON is an array");
    assert!(!checks.is_empty());
    for check in checks {
        assert!(!string_field(check, "name").is_empty());
        assert!(matches!(string_field(check, "status"), "ok" | "warning"));
        assert!(object(check).get("detail").is_some_and(Value::is_string));
    }

    ug(healthy.path())
        .arg("current")
        .assert()
        .code(1)
        .stderr(predicate::str::starts_with("ug: error:"));

    let unhealthy = tempdir().unwrap();
    fs::write(unhealthy.path().join("pending-operation.json"), b"{}\n").unwrap();
    let output = ug(unhealthy.path())
        .args(["--json", "doctor"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    let checks: Value = serde_json::from_slice(&output.stdout).unwrap();
    let pending = checks
        .as_array()
        .unwrap()
        .iter()
        .find(|check| string_field(check, "name") == "pending-operation")
        .expect("doctor reports pending operation");
    assert_eq!(string_field(pending, "status"), "error");
    assert!(object(pending).get("detail").is_some_and(Value::is_string));

    ug(unhealthy.path())
        .args(["--quiet", "doctor"])
        .assert()
        .code(2)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
}

#[test]
fn exec_propagates_the_child_exit_status() {
    let root = tempdir().unwrap();
    let sources = tempdir().unwrap();
    let source = support::fake_godot_with_exit(&sources, "Godot-exit-42", 42);
    ug(root.path())
        .args(["--quiet", "install", "4.7@double", "--from"])
        .arg(source)
        .assert()
        .success();
    ug(root.path())
        .args(["exec", "4.7@double", "--", "ignored"])
        .assert()
        .code(42)
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::is_empty());
}
