use ail_core::config::load;
use ail_core::materialize::materialize;
use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// SPEC §18 — output includes origin annotation per step
#[test]
fn single_file_pipeline_output_has_origin_comment() {
    let pipeline = load(&fixtures_dir().join("minimal.ail.yaml")).unwrap();
    let output = materialize(&pipeline);
    assert!(output.contains("# origin:"));
}

/// SPEC §18 — output is valid parseable YAML
#[test]
fn materialized_output_is_valid_yaml() {
    let pipeline = load(&fixtures_dir().join("minimal.ail.yaml")).unwrap();
    let output = materialize(&pipeline);
    let parsed: serde_yaml::Value = serde_yaml::from_str(&output).unwrap();
    assert!(parsed.is_mapping());
}

/// SPEC §18 — round-trip: materialize → parse → materialize is stable
#[test]
fn materialized_output_round_trips_through_parser() {
    let tmp = tempfile::tempdir().unwrap();
    let pipeline = load(&fixtures_dir().join("minimal.ail.yaml")).unwrap();
    let output = materialize(&pipeline);

    let materialized_path = tmp.path().join("materialized.ail.yaml");
    std::fs::write(&materialized_path, &output).unwrap();

    let pipeline2 = load(&materialized_path).unwrap();
    let output2 = materialize(&pipeline2);

    assert_eq!(pipeline2.steps.len(), 1);
    assert_eq!(pipeline2.steps[0].id.as_str(), "dont_be_stupid");
    assert!(output2.contains("dont_be_stupid"));
}
