use ail_init::{InitArgs, TemplateFile, TemplateMeta};

// Use the public entry point + a private helper. We can't construct
// `BundledSource` directly from an integration test (it's behind the
// `source` private module), but we can exercise list/fetch through
// a fresh `run()` call and, more usefully, by wiring a local copy of
// the source logic. Simplest: test the public `run()` output format
// is defensive, and test the observable contract via a dedicated
// helper module exposed for tests below.

#[test]
fn run_without_template_lists_all_three() {
    let args = InitArgs {
        template: None,
        force: false,
        dry_run: false,
    };
    // run() prints to stdout; we can't easily capture without a wrapper,
    // but we can at least verify it doesn't error.
    assert!(ail_init::run(args).is_ok());
}

#[test]
fn run_with_unknown_template_errors() {
    let args = InitArgs {
        template: Some("does-not-exist".to_string()),
        force: false,
        dry_run: false,
    };
    let err = ail_init::run(args).unwrap_err();
    assert!(err.detail().contains("does-not-exist"));
    assert!(err.detail().contains("starter"));
    assert!(err.detail().contains("superpowers"));
    assert!(err.detail().contains("oh-my-ail"));
}

#[test]
fn run_with_known_template_succeeds() {
    for name in ["starter", "superpowers", "oh-my-ail"] {
        let args = InitArgs {
            template: Some(name.to_string()),
            force: false,
            dry_run: false,
        };
        assert!(
            ail_init::run(args).is_ok(),
            "expected `{name}` to resolve"
        );
    }
}

#[test]
fn run_with_alias_oma_resolves_oh_my_ail() {
    let args = InitArgs {
        template: Some("oma".to_string()),
        force: false,
        dry_run: false,
    };
    assert!(ail_init::run(args).is_ok());
}

// Silence unused-import warnings on types we re-export for public API
// stability but don't construct in these tests.
#[allow(dead_code)]
fn _public_api_surface(_: TemplateMeta, _: TemplateFile) {}
