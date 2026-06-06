#[test]
fn expand_fixtures_compile() {
    let t = trybuild::TestCases::new();
    t.pass("tests/expand/*.rs");
}

#[test]
fn ui_fixtures_report_friendly_errors() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/*.rs");
}
