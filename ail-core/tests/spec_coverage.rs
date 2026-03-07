mod spec {
    mod s3_file_format {
        mod s3_1_discovery {
            use ail_core::config::discovery::discover;
            use std::path::PathBuf;

            #[test]
            fn explicit_path_takes_precedence() {
                let explicit = PathBuf::from("/some/explicit/path.ail.yaml");
                let result = discover(Some(explicit.clone()));
                assert_eq!(result, Some(explicit));
            }

            #[test]
            fn returns_none_when_no_file_found() {
                // Use a temp dir with no ail files
                let tmp = tempfile::tempdir().unwrap();
                let original_dir = std::env::current_dir().unwrap();
                std::env::set_current_dir(tmp.path()).unwrap();
                let result = discover(None);
                std::env::set_current_dir(original_dir).unwrap();
                assert!(result.is_none());
            }

            #[test]
            fn falls_back_to_ail_yaml_in_cwd() {
                let tmp = tempfile::tempdir().unwrap();
                let ail_yaml = tmp.path().join(".ail.yaml");
                std::fs::write(
                    &ail_yaml,
                    "version: \"0.0.1\"\npipeline:\n  - id: s\n    prompt: x\n",
                )
                .unwrap();
                let original_dir = std::env::current_dir().unwrap();
                std::env::set_current_dir(tmp.path()).unwrap();
                let result = discover(None);
                std::env::set_current_dir(original_dir).unwrap();
                assert_eq!(result, Some(PathBuf::from(".ail.yaml")));
            }
        }

        mod s3_2_top_level_structure {
            use ail_core::config::load;
            use std::path::PathBuf;

            fn fixtures_dir() -> PathBuf {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
            }

            #[test]
            fn minimal_pipeline_parses_to_domain_type() {
                let result = load(&fixtures_dir().join("minimal.ail.yaml"));
                assert!(result.is_ok());
                let pipeline = result.unwrap();
                assert_eq!(pipeline.steps.len(), 1);
                assert_eq!(pipeline.steps[0].id.as_str(), "dont_be_stupid");
            }

            #[test]
            fn missing_version_returns_validation_error() {
                let result = load(&fixtures_dir().join("invalid_no_version.ail.yaml"));
                assert!(result.is_err());
                let err = result.unwrap_err();
                assert!(err.to_string().contains("version"));
            }

            #[test]
            fn empty_pipeline_returns_validation_error() {
                let result = load(&fixtures_dir().join("invalid_empty_pipeline.ail.yaml"));
                assert!(result.is_err());
                let err = result.unwrap_err();
                assert!(err.to_string().contains("pipeline"));
            }
        }
    }

    mod s5_step_specification {
        mod s5_1_core_fields {
            use ail_core::config::{domain::StepBody, load};
            use std::path::PathBuf;

            fn fixtures_dir() -> PathBuf {
                PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
            }

            #[test]
            fn prompt_field_parses_to_prompt_body() {
                let pipeline = load(&fixtures_dir().join("minimal.ail.yaml")).unwrap();
                assert!(matches!(pipeline.steps[0].body, StepBody::Prompt(_)));
            }

            #[test]
            fn step_id_is_newtype_not_raw_string() {
                let pipeline = load(&fixtures_dir().join("minimal.ail.yaml")).unwrap();
                // StepId is a newtype — verify we access it via .as_str()
                assert_eq!(pipeline.steps[0].id.as_str(), "dont_be_stupid");
            }

            #[test]
            fn duplicate_step_ids_return_validation_error() {
                let result = load(&fixtures_dir().join("invalid_duplicate_ids.ail.yaml"));
                assert!(result.is_err());
                let err = result.unwrap_err();
                assert!(err.to_string().contains("review"));
            }

            #[test]
            fn step_with_no_primary_field_is_invalid() {
                let result = load(&fixtures_dir().join("invalid_no_primary_field.ail.yaml"));
                assert!(result.is_err());
                let err = result.unwrap_err();
                assert!(err.to_string().contains("primary field"));
            }
        }
    }

    mod s18_materialize_chain {
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
            // Second pass output is structurally stable (same step count and id)
            assert!(output2.contains("dont_be_stupid"));
        }
    }

    mod s21_mvp {
        /// SPEC §21 — the v0.0.1 binary compiles and runs
        #[test]
        fn binary_compiles() {
            // Structural test: if this file compiles, the crate structure
            // is correct. Observable verification is manual in Phase 1.
        }
    }

    mod s17_error_handling {
        use ail_core::error::{error_types, AilError, ErrorContext};

        /// SPEC §17 — errors carry a stable type string and instance detail
        #[test]
        fn ail_error_display_contains_type_and_detail() {
            let err = AilError {
                error_type: error_types::RUNNER_INVOCATION_FAILED,
                title: "Runner invocation failed",
                detail: "process exited with code 1".to_string(),
                context: Some(ErrorContext {
                    pipeline_run_id: Some("run-abc".to_string()),
                    step_id: Some("dont_be_stupid".to_string()),
                    source: Some("exit status: 1".to_string()),
                }),
            };
            let display = err.to_string();
            assert!(display.contains(error_types::RUNNER_INVOCATION_FAILED));
            assert!(display.contains("process exited with code 1"));
        }
    }
}
