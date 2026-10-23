//! Parity-scene entries: reads the list of CLI parity commands and locates the parity directory, the baselines and the repository root.

use super::*;

/// JSON settings leaves the parameter table exposes, as (dotted path, parameter name).
/// `defocus.circle_of_confusion.size` is also set from `--radius`, which overrides it as the CLI does.
pub(super) const EXPOSED: &[(&str, &str)] = &[
    ("defocus.defocus_mode", "defocusMode"),
    ("defocus.size_multiplier", "sizeMultiplier"),
    ("defocus.gamma_correction", "gamma"),
    ("defocus.circle_of_confusion.size", "size"),
    ("defocus.circle_of_confusion.max_size", "maxSize"),
    ("defocus.circle_of_confusion.math", "math"),
    ("defocus.circle_of_confusion.focal_plane", "focalPlane"),
    ("defocus.circle_of_confusion.protect", "protect"),
    (
        "defocus.circle_of_confusion.camera_data.focal_length",
        "focalLength",
    ),
    ("defocus.camera_max_size", "cameraMaxSize"),
    ("defocus.circle_of_confusion.camera_data.f_stop", "fStop"),
    (
        "defocus.circle_of_confusion.camera_data.world_unit",
        "worldUnit",
    ),
    (
        "defocus.circle_of_confusion.camera_data.filmback.width",
        "filmbackWidth",
    ),
    (
        "defocus.circle_of_confusion.camera_data.filmback.height",
        "filmbackHeight",
    ),
];

/// One full-frame row of `commands.tsv`.
pub(super) struct Entry {
    pub(super) name: String,
    pub(super) input: PathBuf,
    pub(super) depth: Option<PathBuf>,
    pub(super) settings: PathBuf,
    pub(super) radius: f32,
    pub(super) quality: i32,
}

impl Entry {
    /// Parses `input [--depth D] --settings S --radius R --quality Q --render-mode M [...]`.
    /// Returns `None` for rows that are not full-frame renders.
    pub(super) fn parse(name: &str, command: &str) -> Option<Self> {
        let mut tokens = command.split_whitespace();
        let input = PathBuf::from(tokens.next()?);
        let mut flags: HashMap<&str, &str> = HashMap::new();
        let rest: Vec<&str> = tokens.collect();
        for pair in rest.chunks(2) {
            if let [flag, value] = pair {
                flags.insert(flag.trim_start_matches("--"), *value);
            }
        }
        if flags.get("render-mode") != Some(&"full") {
            return None;
        }
        let quality = match *flags.get("quality")? {
            "fast" => Quality::Fast as i32,
            "accurate" => Quality::Accurate as i32,
            other => panic!("{name}: unsupported quality {other}"),
        };
        Some(Self {
            name: name.to_owned(),
            input,
            depth: flags.get("depth").map(|v| PathBuf::from(*v)),
            settings: PathBuf::from(*flags.get("settings")?),
            radius: flags.get("radius")?.parse().ok()?,
            quality,
        })
    }
}

/// Returns the directory with `commands.tsv` and the settings the scenes use.
pub(super) fn parity_dir() -> PathBuf {
    repo_root().join("test").join("parity")
}

/// Returns the directory with the baseline EXRs that `cargo xtask parity` renders with the CLI.
pub(super) fn baseline_dir() -> PathBuf {
    repo_root().join("target").join("parity").join("exr")
}

pub(super) fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .canonicalize()
        .expect("repository root")
}

/// Reads `commands.tsv` with `{repo}` and `{parity}` placeholders replaced.
pub(super) fn parity_entries(parity: &Path, repo: &Path) -> Vec<Entry> {
    let text = std::fs::read_to_string(parity.join("commands.tsv")).expect("commands.tsv");
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| {
            let (name, command) = line.split_once('\t').expect("name<TAB>command");
            let command = command
                .replace("{repo}", &repo.display().to_string())
                .replace("{parity}", &parity.display().to_string());
            Entry::parse(name, &command)
        })
        .collect()
}
