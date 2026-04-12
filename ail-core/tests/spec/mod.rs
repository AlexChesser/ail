mod r05_http_runner;
mod s03_file_format;
mod s04_execution_model;
mod s04_log_provider;
mod s05_3_on_result;
mod s05_5_context_steps;
mod s05_append_system_prompt;
mod s05_step_specification;
mod s06_skills;
mod s07_pipeline_inheritance;
mod s08_multi_runner;
mod s08_runner_adapter;
mod s08_subprocess;
mod s09_permission_listener;
mod s09_sub_pipeline;
mod s09_tool_permissions;
mod s10_model_config;
mod s11_template_variables;
mod s12_step_conditions;
mod s13_sqlite_provider;
mod s17_error_handling;
mod s18_materialize;
mod s19_plugin_discovery;
mod s19_plugin_protocol;
mod s21_mvp;
mod s23_structured_output;
mod s35_ail_log_formatter;
mod s39_consistency;
mod s40_delete_run;

/// Serialises all tests that mutate process-wide CWD via `std::env::set_current_dir`.
/// `std::env::current_dir()` is global process state; parallel tests that change it
/// corrupt each other.  Hold this lock for the duration of any test that calls
/// `set_current_dir`, and release it (via drop) before the test returns.
pub static CWD_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
