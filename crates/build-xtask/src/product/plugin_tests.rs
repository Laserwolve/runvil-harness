//! Plugin-family fixtures: a `plugin-*` crate depends only on
//! `runvil-plugin` and outside libraries, and Engine and Harness crates
//! may not depend on a Plugin crate.

use super::product_boundary_violations;
use super::test_support::write_crate;

/// The violations of a workspace holding `plugin-mcp` with `deps`, beside
/// a crate for each workspace dependency it may name.
fn plugin_mcp_naming(deps: &str) -> Vec<String> {
    let root = tempfile::TempDir::new().expect("tempdir");
    write_crate(root.path(), "plugin-mcp", "plugin-mcp", deps);
    for (dir, name) in [
        ("runvil-plugin", "runvil-plugin"),
        ("runvil-engine", "runvil-engine"),
        ("runvil", "runvil"),
        ("plugin-notes", "plugin-notes"),
        ("build-xtask", "build-xtask"),
    ] {
        write_crate(root.path(), dir, name, "");
    }
    product_boundary_violations(root.path())
}

const PLUGIN_RULE: &str = "plugin crates may depend only on runvil-plugin and outside libraries";

#[test]
fn a_plugin_crate_naming_the_contract_passes() {
    let violations = plugin_mcp_naming(
        "[dependencies]\nrunvil-plugin = { path = \"../runvil-plugin\" }\n\
         [dev-dependencies]\nrunvil-plugin = { path = \"../runvil-plugin\", features = [\"test-support\"] }\n",
    );
    assert!(violations.is_empty(), "{violations:?}");
}

#[test]
fn a_plugin_crate_naming_any_other_workspace_crate_is_reported_in_every_table() {
    for (table, dep) in [
        ("dependencies", "runvil-engine"),
        ("dependencies", "runvil"),
        ("dev-dependencies", "plugin-notes"),
        ("build-dependencies", "build-xtask"),
    ] {
        let violations =
            plugin_mcp_naming(&format!("[{table}]\n{dep} = {{ path = \"../{dep}\" }}\n"));
        assert_eq!(violations.len(), 1, "{table} {dep}: {violations:?}");
        assert!(
            violations[0].starts_with(&format!("plugin-mcp depends on {dep}:"))
                && violations[0].contains(PLUGIN_RULE),
            "{table} {dep}: {violations:?}"
        );
    }
}

/// The violations of `package` in `dir` naming `plugin-mcp` under `table`.
fn naming_plugin_mcp(dir: &str, package: &str, table: &str) -> Vec<String> {
    let root = tempfile::TempDir::new().expect("tempdir");
    let depth = "../".repeat(dir.matches('/').count() + 1);
    write_crate(
        root.path(),
        dir,
        package,
        &format!("[{table}]\nplugin-mcp = {{ path = \"{depth}plugin-mcp\" }}\n"),
    );
    write_crate(root.path(), "plugin-mcp", "plugin-mcp", "");
    product_boundary_violations(root.path())
}

#[test]
fn engine_and_harness_crates_naming_a_plugin_crate_are_reported() {
    for (dir, package, table) in [
        ("runvil-engine", "runvil-engine", "dependencies"),
        ("runvil", "runvil", "dev-dependencies"),
        ("harness-internal/runner", "harness-runner", "dependencies"),
        (
            "harness-gateway-client",
            "harness-gateway-client",
            "dependencies",
        ),
    ] {
        let violations = naming_plugin_mcp(dir, package, table);
        assert_eq!(violations.len(), 1, "{package}: {violations:?}");
        assert!(
            violations[0].starts_with(&format!("{package} depends on plugin-mcp:"))
                && violations[0]
                    .contains("Engine and harness crates must not depend on plugin crates"),
            "{package}: {violations:?}"
        );
    }
}

#[test]
fn an_outside_crate_may_name_a_plugin_crate() {
    let violations = naming_plugin_mcp("outside-tool", "outside-tool", "dependencies");
    assert!(violations.is_empty(), "{violations:?}");
}
