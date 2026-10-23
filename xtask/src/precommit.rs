//! Pre-commit and repository policy checks: Markdown placement, lint inheritance, the
//! AGENTS.md code rules, type checks, Rust formatting and clippy, and Ruff for the Natron
//! scripts.

use anyhow::{Result, bail};
use duct::cmd;

use crate::compile::validate_gpu_shaders;
use crate::util::crate_root;

/// Runs every pre-commit check in order. `args` are the staged file paths: they narrow the
/// Rust and Ruff checks to the packages and scripts they touch, and a path containing
/// `kernel` also validates the GPU shaders. Returns the first failing check's error.
pub async fn precommit(args: Vec<String>) -> Result<()> {
    check_markdown_policy()?;
    check_lint_inheritance()?;
    check_code_rules()?;
    run_ty_typechecking()?;
    run_rust_checks(&args)?;
    run_ruff(&args)?;
    if args.iter().any(|arg| arg.contains("kernel")) {
        validate_gpu_shaders().await?;
    }
    Ok(())
}

/// Runs `ty check` for the circle-of-confusion Python project after syncing its dev
/// dependencies. Fails when either command fails.
fn run_ty_typechecking() -> Result<()> {
    {
        let project = "crates/circle-of-confusion";
        let dir = crate_root().join(project);
        // ty only resolves test imports such as pytest from the project's own dev environment.
        cmd!("uv", "sync", "--no-install-project", "--group", "dev")
            .dir(&dir)
            .run()?;
        cmd!("uv", "run", "--no-sync", "ty", "check")
            .dir(&dir)
            .run()?;
    }
    Ok(())
}

/// Markdown files that may sit in the repository root.
const ROOT_MARKDOWN: [&str; 5] = [
    "README.md",
    "AGENTS.md",
    "ARCHITECTURE.md",
    "CONTRIBUTING.md",
    "CHANGELOG.md",
];

/// Whether a tracked Markdown path is in an allowed location: a root document, `docs/src/`,
/// a test image README, `xtask/README.md`, or a crate-level README or CHANGELOG.
fn markdown_allowed(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    ROOT_MARKDOWN.contains(&path)
        || path.starts_with("docs/src/")
        || path.starts_with("test/images/") && name == "README.md"
        || path == "xtask/README.md"
        || path.starts_with("crates/")
            && path.matches('/').count() == 2
            && (name == "README.md" || name == "CHANGELOG.md")
}

/// Keeps agent notes, handoffs and plans out of the repository (see AGENTS.md).
fn check_markdown_policy() -> Result<()> {
    let listing = cmd!("git", "ls-files", "--cached", "--", "*.md")
        .dir(crate_root())
        .read()?;
    let rejected: Vec<&str> = listing.lines().filter(|p| !markdown_allowed(p)).collect();
    if !rejected.is_empty() {
        bail!(
            "markdown outside the allowed locations (see 'Where knowledge goes' in AGENTS.md):\n  {}",
            rejected.join("\n  ")
        );
    }
    Ok(())
}

/// Whether the file name of `file` ends with `extension`, ignoring ASCII case.
fn has_extension(file: &str, extension: &str) -> bool {
    std::path::Path::new(file)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(extension))
}

/// Files whose change needs a Rust format and lint run.
fn rust_related(file: &str) -> bool {
    has_extension(file, "rs")
        || ["Cargo.toml", "Cargo.lock", "clippy.toml"]
            .iter()
            .any(|name| file.rsplit('/').next() == Some(name))
}

/// Workspace packages (name, directory) from the root manifest.
fn workspace_packages() -> Result<Vec<(String, String)>> {
    let root = crate_root();
    let manifest: toml::Table = std::fs::read_to_string(root.join("Cargo.toml"))?.parse()?;
    let mut packages = Vec::new();
    for member in manifest["workspace"]["members"]
        .as_array()
        .into_iter()
        .flatten()
    {
        let Some(dir) = member.as_str() else { continue };
        let package: toml::Table =
            std::fs::read_to_string(root.join(dir).join("Cargo.toml"))?.parse()?;
        if let Some(name) = package["package"]["name"].as_str() {
            packages.push((name.to_owned(), dir.to_owned()));
        }
    }
    Ok(packages)
}

/// Packages to lint for the staged files; `None` means the whole workspace.
fn packages_to_lint(files: &[String], packages: &[(String, String)]) -> Option<Vec<String>> {
    let mut selected = Vec::new();
    for file in files.iter().filter(|f| rust_related(f)) {
        let owner = packages
            .iter()
            .find(|(_, dir)| file.starts_with(&format!("{dir}/")));
        match owner {
            Some((name, _)) => {
                if !selected.contains(name) {
                    selected.push(name.clone());
                }
            }
            None => return None,
        }
    }
    Some(selected)
}

/// Formats and lints only the Rust packages that the staged files belong to.
fn run_rust_checks(files: &[String]) -> Result<()> {
    if !files.iter().any(|f| rust_related(f)) {
        return Ok(());
    }
    let root = crate_root();
    cmd!("cargo", "fmt", "--all", "--", "--check")
        .dir(&root)
        .run()?;
    match packages_to_lint(files, &workspace_packages()?) {
        Some(selected) => selected.iter().try_for_each(|name| clippy(Some(name))),
        None => clippy_all(),
    }
}

/// Runs `cargo clippy --all-targets --locked -- -D warnings` for one package, or for the
/// whole workspace when `package` is `None`. Fails when clippy reports a warning or fails
/// to build.
fn clippy(package: Option<&str>) -> Result<()> {
    let scope = package.map_or(vec!["--workspace"], |name| vec!["-p", name]);
    cmd(
        "cargo",
        ["clippy", "--all-targets", "--locked"]
            .into_iter()
            .chain(scope)
            .chain(["--", "-D", "warnings"]),
    )
    .dir(crate_root())
    .run()?;
    Ok(())
}

/// Lints the workspace, then every package on its own. Feature unification differs
/// between the two, so a package can compile with a feature set (for example
/// `bokeh-creator` without `noise` under `opendefocus`) that the workspace build hides.
///
/// # Errors
/// Returns an error when any clippy run reports a warning or fails to build.
pub fn clippy_all() -> Result<()> {
    clippy(None)?;
    for (name, _) in workspace_packages()? {
        clippy(Some(&name))?;
    }
    Ok(())
}

/// Ruff for the Natron host scripts.
fn run_ruff(files: &[String]) -> Result<()> {
    let python = files
        .iter()
        .any(|f| has_extension(f, "py") && f.starts_with("test/natron/"));
    if !python {
        return Ok(());
    }
    let root = crate_root();
    for sub in [vec!["check"], vec!["format", "--check"]] {
        cmd(
            "uvx",
            ["--from", "ruff==0.14.4", "ruff"]
                .into_iter()
                .chain(sub)
                .chain(["test/natron"]),
        )
        .dir(&root)
        .run()?;
    }
    Ok(())
}

/// Maximum number of lines in a Rust file, tests included.
const MAX_RUST_LINES: usize = 800;
/// Characters that make a run of three or more on a `//` line a banner comment.
const BANNER_CHARS: [char; 6] = ['-', '=', '#', '*', '~', '_'];
/// Extensions of script files that are only allowed under `NON_RUST_CODE_HOMES`.
const NON_RUST_CODE_EXTENSIONS: [&str; 6] = ["py", "sh", "ps1", "bat", "js", "ts"];
/// Directories where non-Rust code is allowed, as described in AGENTS.md.
const NON_RUST_CODE_HOMES: [&str; 4] = [
    "crates/circle-of-confusion/python/",
    "crates/circle-of-confusion/test/python/",
    "test/natron/",
    "packaging/",
];

/// Whether a line is a `//` banner such as `// ----` or `// === Section ===`. Doc comments
/// (`///` and `//!`) never count.
fn is_banner_comment(line: &str) -> bool {
    let Some(text) = line.trim_start().strip_prefix("//") else {
        return false;
    };
    if text.starts_with(['/', '!']) {
        return false;
    }
    let text = text.trim();
    text.chars()
        .next()
        .is_some_and(|c| BANNER_CHARS.contains(&c) && text.chars().take(3).all(|d| d == c))
}

/// Rules from AGENTS.md: no banner comments, small Rust files, no scripts outside Rust.
fn code_rule_violations(files: &[(String, Option<String>)]) -> Vec<String> {
    let mut violations = Vec::new();
    for (path, content) in files {
        if let Some(content) = content.as_deref().filter(|_| has_extension(path, "rs")) {
            let lines = content.lines().count();
            if lines > MAX_RUST_LINES {
                violations.push(format!(
                    "{path}: {lines} lines, split it (max {MAX_RUST_LINES})"
                ));
            }
            if !content.trim_start().starts_with("//!") {
                violations.push(format!(
                    "{path}: missing `//!` description at the top of the file"
                ));
            }
            for (number, line) in content.lines().enumerate() {
                if is_banner_comment(line) {
                    violations.push(format!(
                        "{path}:{}: banner comment, use submodules",
                        number + 1
                    ));
                }
            }
        }
        let script = NON_RUST_CODE_EXTENSIONS
            .iter()
            .any(|e| has_extension(path, e));
        if script
            && !NON_RUST_CODE_HOMES
                .iter()
                .any(|home| path.starts_with(home))
        {
            violations.push(format!(
                "{path}: scripts are not allowed, add an xtask command"
            ));
        }
    }
    violations
}

/// Checks the tracked files against the AGENTS.md code rules and fails with every
/// violation listed.
fn check_code_rules() -> Result<()> {
    let root = crate_root();
    let listing = cmd!("git", "ls-files", "--cached").dir(&root).read()?;
    let files: Vec<(String, Option<String>)> = listing
        .lines()
        .map(|path| {
            (
                path.to_owned(),
                std::fs::read_to_string(root.join(path)).ok(),
            )
        })
        .collect();
    let violations = code_rule_violations(&files);
    if !violations.is_empty() {
        bail!(
            "code rules from AGENTS.md violated:\n  {}",
            violations.join("\n  ")
        );
    }
    Ok(())
}

/// Every workspace member must inherit the workspace lints (strict clippy).
fn check_lint_inheritance() -> Result<()> {
    let root = crate_root();
    let manifest: toml::Table = std::fs::read_to_string(root.join("Cargo.toml"))?.parse()?;
    let members = manifest["workspace"]["members"]
        .as_array()
        .map(|m| m.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>())
        .unwrap_or_default();
    let missing: Vec<&str> = members
        .into_iter()
        .filter(|member| {
            std::fs::read_to_string(root.join(member).join("Cargo.toml"))
                .ok()
                .and_then(|s| s.parse::<toml::Table>().ok())
                .and_then(|t| t.get("lints")?.get("workspace")?.as_bool())
                != Some(true)
        })
        .collect();
    if !missing.is_empty() {
        bail!(
            "workspace members without `[lints] workspace = true`:\n  {}",
            missing.join("\n  ")
        );
    }
    Ok(())
}

/// Tests for the repository policy checks.
#[cfg(test)]
mod tests {
    use super::markdown_allowed;

    /// Verifies each code rule: banner comments, the line limit, the missing `//!` header and
    /// scripts outside their allowed directories.
    #[test]
    fn code_rules() {
        let file = |p: &str, c: &str| (p.to_owned(), Some(c.to_owned()));
        let long = "x\n".repeat(801);
        let found = super::code_rule_violations(&[
            file("a.rs", "//! A.\n// ----- Section -----\nfn f() {}\n"),
            file("b.rs", "//! B.\n/// ---\n//! ===\n// plain comment\n"),
            file("c.rs", &format!("//! C.\n{long}")),
            file("d.rs", "fn f() {}\n"),
            file("scripts/x.py", ""),
            file("test/natron/x.py", ""),
        ]);
        assert_eq!(found.len(), 4, "{found:?}");
    }

    /// Verifies that only the packages that own the staged Rust files are linted, and that a
    /// change to the root manifest lints the whole workspace.
    #[test]
    fn lint_scope_follows_staged_files() {
        let pk = |n: &str, d: &str| (n.to_owned(), d.to_owned());
        let packages = [pk("a", "crates/a"), pk("b", "crates/b")];
        let files = |l: &[&str]| l.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            super::packages_to_lint(
                &files(&["crates/a/src/x.rs", "crates/a/Cargo.toml"]),
                &packages
            ),
            Some(vec!["a".to_owned()])
        );
        assert_eq!(
            super::packages_to_lint(&files(&["README.md"]), &packages),
            Some(vec![])
        );
        assert_eq!(
            super::packages_to_lint(&files(&["Cargo.toml"]), &packages),
            None
        );
    }

    /// Verifies which Markdown paths are allowed.
    #[test]
    fn markdown_policy() {
        for ok in [
            "AGENTS.md",
            "docs/src/a/b.md",
            "crates/opendefocus/README.md",
            "test/images/scenes/README.md",
        ] {
            assert!(markdown_allowed(ok), "{ok}");
        }
        for bad in [
            "docs/agent-handoff.md",
            "docs/plan.md",
            "crates/opendefocus/NOTES.md",
            "HANDOFF.md",
            "crates/a/src/x.md",
        ] {
            assert!(!markdown_allowed(bad), "{bad}");
        }
    }
}
