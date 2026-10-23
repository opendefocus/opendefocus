//! Reads the scenes to render from the parity command list, so the preview and the parity
//! test use the same definitions.
use anyhow::{Context, Result, ensure};
use std::{fs, path::Path};

/// Suffix of the single-pass rows of `commands.tsv`; striped rows render the same scene again.
const FULL_FRAME_SUFFIX: &str = "_full";

/// One CLI render: a name for the output files and the arguments that select the scene.
#[derive(Debug, PartialEq)]
pub struct Workload {
    /// Unique name taken from `commands.tsv`, used as the file stem of the outputs.
    pub name: String,
    /// CLI arguments with the repository paths filled in, without output options.
    pub args: Vec<String>,
}

/// Parses `commands.tsv`: tab separated name and arguments, with `{repo}` and `{parity}`
/// standing for the repository root and the parity folder. Only full-frame rows are kept.
///
/// # Errors
/// Fails when a row has no arguments or no row is left.
fn parse(tsv: &str, repo: &Path, parity: &Path) -> Result<Vec<Workload>> {
    let mut workloads = Vec::new();
    for line in tsv.lines().filter(|line| !line.trim().is_empty()) {
        let (name, arguments) = line
            .split_once('\t')
            .with_context(|| format!("Row without arguments: {line}"))?;
        if !name.ends_with(FULL_FRAME_SUFFIX) {
            continue;
        }
        let arguments = arguments
            .replace("{repo}", &repo.to_string_lossy())
            .replace("{parity}", &parity.to_string_lossy());
        workloads.push(Workload {
            name: name.to_owned(),
            args: arguments.split_whitespace().map(str::to_owned).collect(),
        });
    }
    ensure!(!workloads.is_empty(), "No full-frame rows in commands.tsv");
    Ok(workloads)
}

/// Loads the workloads from `test/parity/commands.tsv` below the repository `root`.
///
/// # Errors
/// Fails when the file cannot be read or is malformed.
pub fn load(root: &Path) -> Result<Vec<Workload>> {
    let parity = root.join("test/parity");
    let path = parity.join("commands.tsv");
    let tsv = fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
    parse(&tsv, root, &parity)
}

/// Tests for the command list parser.
#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies that placeholders are filled in and striped rows are skipped.
    #[test]
    fn test_full_rows_are_kept_and_paths_filled_in() {
        let tsv = "a_full\t{repo}/c.exr --settings {parity}/s.json\na_striped\t{repo}/c.exr\n";
        let workloads = parse(tsv, Path::new("/r"), Path::new("/r/p")).unwrap();
        assert_eq!(
            workloads,
            vec![Workload {
                name: "a_full".into(),
                args: vec!["/r/c.exr".into(), "--settings".into(), "/r/p/s.json".into()],
            }]
        );
    }

    /// Verifies that a list without full-frame rows is an error.
    #[test]
    fn test_a_list_without_full_rows_is_rejected() {
        assert!(parse("a_striped\tx\n", Path::new("."), Path::new(".")).is_err());
        assert!(parse("broken\n", Path::new("."), Path::new(".")).is_err());
    }
}
