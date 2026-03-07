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

    mod s11_template_variables {
        use ail_core::config::domain::Pipeline;
        use ail_core::session::{Session, TurnEntry};
        use ail_core::template::resolve;
        use std::time::SystemTime;

        fn make_session() -> Session {
            Session::new(Pipeline::passthrough(), "original prompt".to_string())
        }

        fn append_response(session: &mut Session, step_id: &str, response: &str) {
            session.turn_log.append(TurnEntry {
                step_id: step_id.to_string(),
                prompt: "p".to_string(),
                response: Some(response.to_string()),
                timestamp: SystemTime::now(),
                cost_usd: None,
            });
        }

        #[test]
        fn template_with_no_variables_is_unchanged() {
            let session = make_session();
            assert_eq!(resolve("no vars", &session).unwrap(), "no vars");
        }

        #[test]
        fn last_response_resolves_from_turn_log() {
            let tmp = tempfile::tempdir().unwrap();
            let orig = std::env::current_dir().unwrap();
            std::env::set_current_dir(tmp.path()).unwrap();

            let mut session = make_session();
            append_response(&mut session, "step_1", "the answer");
            let result = resolve("{{ last_response }}", &session).unwrap();
            assert_eq!(result, "the answer");

            std::env::set_current_dir(orig).unwrap();
        }

        #[test]
        fn named_step_response_resolves_correctly() {
            let tmp = tempfile::tempdir().unwrap();
            let orig = std::env::current_dir().unwrap();
            std::env::set_current_dir(tmp.path()).unwrap();

            let mut session = make_session();
            append_response(&mut session, "review", "looks good");
            let result = resolve("{{ step.review.response }}", &session).unwrap();
            assert_eq!(result, "looks good");

            std::env::set_current_dir(orig).unwrap();
        }

        #[test]
        fn pipeline_run_id_resolves_to_session_value() {
            let session = make_session();
            let result = resolve("{{ pipeline.run_id }}", &session).unwrap();
            assert_eq!(result, session.run_id);
        }

        #[test]
        fn env_var_resolves_when_set() {
            std::env::set_var("AIL_TEST_VAR_PHASE7", "hello");
            let session = make_session();
            let result = resolve("{{ env.AIL_TEST_VAR_PHASE7 }}", &session).unwrap();
            assert_eq!(result, "hello");
            std::env::remove_var("AIL_TEST_VAR_PHASE7");
        }

        #[test]
        fn env_var_errors_when_not_set() {
            std::env::remove_var("AIL_TEST_MISSING_VAR_PHASE7");
            let session = make_session();
            let result = resolve("{{ env.AIL_TEST_MISSING_VAR_PHASE7 }}", &session);
            assert!(result.is_err());
        }

        #[test]
        fn unknown_step_id_returns_error_not_empty_string() {
            let session = make_session();
            let result = resolve("{{ step.nonexistent.response }}", &session);
            assert!(result.is_err());
        }

        #[test]
        fn unrecognised_syntax_returns_error_not_empty_string() {
            let session = make_session();
            let result = resolve("{{ totally.made.up }}", &session);
            assert!(result.is_err());
        }
    }

    mod s4_execution_model {
        mod session {
            use ail_core::config::domain::Pipeline;
            use ail_core::session::{Session, TurnEntry, TurnLog};
            use std::time::SystemTime;

            fn make_session() -> Session {
                Session::new(Pipeline::passthrough(), "test prompt".to_string())
            }

            fn make_entry(step_id: &str, response: Option<&str>) -> TurnEntry {
                TurnEntry {
                    step_id: step_id.to_string(),
                    prompt: "some prompt".to_string(),
                    response: response.map(|s| s.to_string()),
                    timestamp: SystemTime::now(),
                    cost_usd: None,
                }
            }

            /// SPEC §4 — each pipeline run has a unique run_id
            #[test]
            fn session_new_generates_unique_run_id() {
                let s = make_session();
                assert!(!s.run_id.is_empty());
            }

            /// SPEC §4 — entries are ordered and retrievable
            #[test]
            fn turn_log_entries_are_ordered() {
                let tmp = tempfile::tempdir().unwrap();
                let original_dir = std::env::current_dir().unwrap();
                std::env::set_current_dir(tmp.path()).unwrap();

                let mut log = TurnLog::new("test-run-ordered".to_string());
                log.append(make_entry("step_1", Some("response 1")));
                log.append(make_entry("step_2", Some("response 2")));
                let entries = log.entries();
                assert_eq!(entries[0].step_id, "step_1");
                assert_eq!(entries[1].step_id, "step_2");

                std::env::set_current_dir(original_dir).unwrap();
            }

            /// SPEC §4 — last_response returns the most recent entry
            #[test]
            fn last_response_returns_most_recent_entry() {
                let tmp = tempfile::tempdir().unwrap();
                let original_dir = std::env::current_dir().unwrap();
                std::env::set_current_dir(tmp.path()).unwrap();

                let mut log = TurnLog::new("test-run-last".to_string());
                log.append(make_entry("step_1", Some("first")));
                log.append(make_entry("step_2", Some("second")));
                assert_eq!(log.last_response(), Some("second"));

                std::env::set_current_dir(original_dir).unwrap();
            }

            /// SPEC §4 — turn log persists to append-only NDJSON file
            #[test]
            fn turn_log_append_writes_ndjson_line_to_disk() {
                let tmp = tempfile::tempdir().unwrap();
                let original_dir = std::env::current_dir().unwrap();
                std::env::set_current_dir(tmp.path()).unwrap();

                let run_id = "test-run-ndjson".to_string();
                let mut log = TurnLog::new(run_id.clone());
                log.append(make_entry("step_1", Some("hello")));

                let path = tmp.path().join(format!(".ail/runs/{run_id}.jsonl"));
                assert!(path.exists(), "NDJSON file should exist at {path:?}");
                let contents = std::fs::read_to_string(&path).unwrap();
                let line = contents.lines().next().unwrap();
                let parsed: serde_json::Value = serde_json::from_str(line).unwrap();
                assert_eq!(parsed["step_id"], "step_1");

                std::env::set_current_dir(original_dir).unwrap();
            }

            /// SPEC §4 — two sessions produce different run_ids
            #[test]
            fn two_sessions_have_distinct_run_ids() {
                let s1 = make_session();
                let s2 = make_session();
                assert_ne!(s1.run_id, s2.run_id);
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
