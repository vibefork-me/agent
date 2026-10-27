//! Build recipes by ecosystem: the command, where its outputs are, what it needs.

use std::path::Path;

/// Ecosystem of a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecosystem {
    /// Gradle (with or without wrapper).
    Gradle,
    /// Maven.
    Maven,
    /// Node with a `build` script.
    Node,
    /// Cargo.
    Rust,
    /// Go.
    Go,
    /// Python with `pyproject.toml`, `setup.py`, or `requirements.txt`.
    Python,
}

impl Ecosystem {
    /// Name for messages.
    pub fn name(self) -> &'static str {
        match self {
            Self::Gradle => "gradle",
            Self::Maven => "maven",
            Self::Node => "node",
            Self::Rust => "rust",
            Self::Go => "go",
            Self::Python => "python",
        }
    }

    /// Whether the output runs on any OS (JVM jars, Node scripts, Python wheels/scripts).
    pub fn portable(self) -> bool {
        matches!(self, Self::Gradle | Self::Maven | Self::Node | Self::Python)
    }
}

/// One step of a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Program (relative to the project for wrappers, or in PATH).
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
}

impl Step {
    /// Creates a step with the given program and arguments.
    pub fn new(program: impl Into<String>, args: &[&str]) -> Self {
        Self {
            program: program.into(),
            args: args.iter().map(|s| (*s).to_owned()).collect(),
        }
    }
}

/// How to build a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    /// Ecosystem.
    pub ecosystem: Ecosystem,
    /// Steps executed in order (e.g. installing dependencies, then building).
    pub steps: Vec<Step>,
    /// Folders (relative) holding the outputs.
    pub outputs: Vec<String>,
}

impl Recipe {
    /// Program of the primary / first step.
    pub fn program(&self) -> &str {
        self.steps
            .first()
            .map(|s| s.program.as_str())
            .unwrap_or_default()
    }

    /// Arguments of the primary / first step.
    pub fn args(&self) -> &[String] {
        self.steps
            .first()
            .map(|s| s.args.as_slice())
            .unwrap_or_default()
    }
}

/// Relative path to the Python executable inside a project's `.venv`.
pub fn venv_python(windows: bool) -> &'static str {
    if windows {
        r".venv\Scripts\python.exe"
    } else {
        ".venv/bin/python"
    }
}

/// The recipe of `dir`, if the project type is known.
pub fn detect_recipe(dir: &Path) -> Option<Recipe> {
    detect_recipe_os(dir, cfg!(windows))
}

/// Detects recipe with explicit operating system convention for wrappers and binary paths.
pub fn detect_recipe_os(dir: &Path, windows: bool) -> Option<Recipe> {
    let has = |f: &str| dir.join(f).exists();
    let r = |ecosystem, program: &str, args: &[&str], outputs: &[&str]| Recipe {
        ecosystem,
        steps: vec![Step::new(program, args)],
        outputs: outputs.iter().map(|s| (*s).to_owned()).collect(),
    };
    let wrapper = |name: &str| {
        if windows {
            format!("{name}.bat")
        } else {
            format!("./{name}")
        }
    };
    if has("gradlew") || has("gradlew.bat") {
        return Some(r(
            Ecosystem::Gradle,
            &wrapper("gradlew"),
            &["build", "-x", "test", "--no-daemon", "-q"],
            &["build/libs", "build/distributions"],
        ));
    }
    if has("build.gradle") || has("build.gradle.kts") {
        return Some(r(
            Ecosystem::Gradle,
            "gradle",
            &["build", "-x", "test", "--no-daemon", "-q"],
            &["build/libs", "build/distributions"],
        ));
    }
    if has("pom.xml") {
        let program = if has("mvnw") {
            wrapper("mvnw")
        } else {
            "mvn".into()
        };
        return Some(r(
            Ecosystem::Maven,
            &program,
            &["-q", "package", "-DskipTests"],
            &["target"],
        ));
    }
    if has("Cargo.toml") {
        return Some(r(
            Ecosystem::Rust,
            "cargo",
            &["build", "--release"],
            &["target/release"],
        ));
    }
    if has("go.mod") {
        return Some(r(
            Ecosystem::Go,
            "go",
            &["build", "-o", "dist/", "./..."],
            &["dist"],
        ));
    }
    if has("package.json") {
        let pkg: serde_json::Value = std::fs::read_to_string(dir.join("package.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if pkg["scripts"]["build"].is_string() {
            let pm = pkg["packageManager"].as_str().unwrap_or_default();
            let mut steps = Vec::new();
            if has("pnpm-lock.yaml") || pm.starts_with("pnpm") {
                let install_args = if has("pnpm-lock.yaml") {
                    &["pnpm", "install", "--frozen-lockfile"][..]
                } else {
                    &["pnpm", "install"][..]
                };
                steps.push(Step::new("corepack", install_args));
                steps.push(Step::new("corepack", &["pnpm", "run", "build"]));
            } else if has("yarn.lock") || pm.starts_with("yarn") {
                steps.push(Step::new("corepack", &["yarn", "install"]));
                steps.push(Step::new("corepack", &["yarn", "run", "build"]));
            } else if has("package-lock.json") || has("npm-shrinkwrap.json") {
                steps.push(Step::new("npm", &["ci"]));
                steps.push(Step::new("npm", &["run", "build"]));
            } else {
                steps.push(Step::new("npm", &["install"]));
                steps.push(Step::new("npm", &["run", "build"]));
            }
            return Some(Recipe {
                ecosystem: Ecosystem::Node,
                steps,
                outputs: vec!["dist".into(), "build".into()],
            });
        }
    }
    if has("pyproject.toml") || has("setup.py") || has("requirements.txt") {
        let py = venv_python(windows);
        let mut steps = Vec::new();
        if !has(".venv") {
            steps.push(Step::new("python", &["-m", "venv", ".venv"]));
        }
        if has("requirements.txt") {
            steps.push(Step::new(
                py,
                &[
                    "-m",
                    "pip",
                    "install",
                    "--disable-pip-version-check",
                    "--no-input",
                    "-r",
                    "requirements.txt",
                ],
            ));
        } else {
            steps.push(Step::new(
                py,
                &[
                    "-m",
                    "pip",
                    "install",
                    "--disable-pip-version-check",
                    "--no-input",
                    ".",
                ],
            ));
        }
        if has("pyproject.toml") || has("setup.py") {
            steps.push(Step::new(
                py,
                &[
                    "-m",
                    "pip",
                    "wheel",
                    "--disable-pip-version-check",
                    "--no-input",
                    "--no-deps",
                    "-w",
                    "dist",
                    ".",
                ],
            ));
        }
        return Some(Recipe {
            ecosystem: Ecosystem::Python,
            steps,
            outputs: vec!["dist".into()],
        });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with_os(files: &[(&str, &str)], windows: bool) -> Option<Recipe> {
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!(
            "vf-recipe-{}-{nanos}-{counter}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        for (f, c) in files {
            let p = dir.join(f);
            if let Some(parent) = p.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(p, c);
        }
        let r = detect_recipe_os(&dir, windows);
        let _ = std::fs::remove_dir_all(&dir);
        r
    }

    fn with(files: &[(&str, &str)]) -> Option<Recipe> {
        with_os(files, cfg!(windows))
    }

    #[test]
    fn recipes() {
        assert_eq!(
            with(&[("gradlew", ""), ("build.gradle", "")]).map(|r| r.ecosystem),
            Some(Ecosystem::Gradle)
        );
        assert_eq!(
            with(&[("pom.xml", "")]).as_ref().map(|r| r.program()),
            Some("mvn")
        );
        assert_eq!(
            with(&[("Cargo.toml", "")]).map(|r| r.outputs),
            Some(vec!["target/release".into()])
        );
        assert_eq!(
            with(&[("package.json", r#"{"scripts":{"build":"tsc"}}"#)]).map(|r| r.ecosystem),
            Some(Ecosystem::Node)
        );
        assert_eq!(
            with(&[("package.json", r#"{"scripts":{"start":"node ."}}"#)]),
            None,
            "nothing to build"
        );
        assert_eq!(with(&[("README.md", "")]), None);
    }

    #[test]
    fn node_recipe_installs_dependencies_with_lockfile_managers() {
        // npm ci when package-lock.json exists
        let r_npm = with(&[
            ("package.json", r#"{"scripts":{"build":"vite build"}}"#),
            ("package-lock.json", "{}"),
        ])
        .unwrap();
        assert_eq!(r_npm.ecosystem, Ecosystem::Node);
        assert_eq!(r_npm.steps.len(), 2);
        assert_eq!(r_npm.steps[0], Step::new("npm", &["ci"]));
        assert_eq!(r_npm.steps[1], Step::new("npm", &["run", "build"]));

        // pnpm via corepack when pnpm-lock.yaml exists
        let r_pnpm = with(&[
            ("package.json", r#"{"scripts":{"build":"vite build"}}"#),
            ("pnpm-lock.yaml", "lockfileVersion: 9.0"),
        ])
        .unwrap();
        assert_eq!(r_pnpm.ecosystem, Ecosystem::Node);
        assert_eq!(r_pnpm.steps.len(), 2);
        assert_eq!(
            r_pnpm.steps[0],
            Step::new("corepack", &["pnpm", "install", "--frozen-lockfile"])
        );
        assert_eq!(
            r_pnpm.steps[1],
            Step::new("corepack", &["pnpm", "run", "build"])
        );

        // yarn via corepack when yarn.lock exists
        let r_yarn = with(&[
            ("package.json", r#"{"scripts":{"build":"vite build"}}"#),
            ("yarn.lock", ""),
        ])
        .unwrap();
        assert_eq!(r_yarn.ecosystem, Ecosystem::Node);
        assert_eq!(r_yarn.steps.len(), 2);
        assert_eq!(r_yarn.steps[0], Step::new("corepack", &["yarn", "install"]));
        assert_eq!(
            r_yarn.steps[1],
            Step::new("corepack", &["yarn", "run", "build"])
        );

        // npm install fallback when no lockfile exists
        let r_fallback =
            with(&[("package.json", r#"{"scripts":{"build":"vite build"}}"#)]).unwrap();
        assert_eq!(r_fallback.ecosystem, Ecosystem::Node);
        assert_eq!(r_fallback.steps.len(), 2);
        assert_eq!(r_fallback.steps[0], Step::new("npm", &["install"]));
        assert_eq!(r_fallback.steps[1], Step::new("npm", &["run", "build"]));
    }

    #[test]
    fn venv_python_paths_for_windows_and_unix() {
        assert_eq!(venv_python(true), r".venv\Scripts\python.exe");
        assert_eq!(venv_python(false), ".venv/bin/python");
    }

    #[test]
    fn python_recipe_detects_pyproject_and_requirements() {
        // Windows path test
        let r_win = with_os(
            &[
                (
                    "pyproject.toml",
                    "[project]\nname = 'demo'\nversion = '0.1.0'",
                ),
                ("requirements.txt", "requests>=2.0"),
            ],
            true,
        )
        .unwrap();
        assert_eq!(r_win.ecosystem, Ecosystem::Python);
        assert_eq!(r_win.outputs, vec!["dist"]);
        assert_eq!(r_win.steps.len(), 3);
        assert_eq!(
            r_win.steps[0],
            Step::new("python", &["-m", "venv", ".venv"])
        );
        assert_eq!(
            r_win.steps[1],
            Step::new(
                r".venv\Scripts\python.exe",
                &[
                    "-m",
                    "pip",
                    "install",
                    "--disable-pip-version-check",
                    "--no-input",
                    "-r",
                    "requirements.txt"
                ]
            )
        );
        assert_eq!(
            r_win.steps[2],
            Step::new(
                r".venv\Scripts\python.exe",
                &[
                    "-m",
                    "pip",
                    "wheel",
                    "--disable-pip-version-check",
                    "--no-input",
                    "--no-deps",
                    "-w",
                    "dist",
                    "."
                ]
            )
        );

        // Unix path test
        let r_unix = with_os(
            &[
                (
                    "pyproject.toml",
                    "[project]\nname = 'demo'\nversion = '0.1.0'",
                ),
                ("requirements.txt", "requests>=2.0"),
            ],
            false,
        )
        .unwrap();
        assert_eq!(r_unix.steps[1].program, ".venv/bin/python");
        assert_eq!(r_unix.steps[2].program, ".venv/bin/python");

        // When .venv already exists, python -m venv step is omitted
        let r_existing_venv = with_os(
            &[
                (
                    "pyproject.toml",
                    "[project]\nname = 'demo'\nversion = '0.1.0'",
                ),
                (".venv/pyvenv.cfg", "home = /usr/bin"),
            ],
            false,
        )
        .unwrap();
        assert_eq!(r_existing_venv.steps.len(), 2);
        assert_eq!(r_existing_venv.steps[0].program, ".venv/bin/python");
        assert_eq!(r_existing_venv.steps[0].args[1], "pip");

        // requirements.txt alone without .venv
        let r_req = with_os(&[("requirements.txt", "flask>=3.0")], false).unwrap();
        assert_eq!(r_req.ecosystem, Ecosystem::Python);
        assert_eq!(r_req.steps.len(), 2);
        assert_eq!(
            r_req.steps[0],
            Step::new("python", &["-m", "venv", ".venv"])
        );
        assert_eq!(
            r_req.steps[1],
            Step::new(
                ".venv/bin/python",
                &[
                    "-m",
                    "pip",
                    "install",
                    "--disable-pip-version-check",
                    "--no-input",
                    "-r",
                    "requirements.txt"
                ]
            )
        );

        // setup.py alone
        let r_setup = with_os(
            &[("setup.py", "from setuptools import setup; setup()")],
            false,
        )
        .unwrap();
        assert_eq!(r_setup.ecosystem, Ecosystem::Python);
        assert_eq!(r_setup.steps.len(), 3);
        assert_eq!(
            r_setup.steps[0],
            Step::new("python", &["-m", "venv", ".venv"])
        );
        assert_eq!(r_setup.steps[1].program, ".venv/bin/python");
        assert_eq!(r_setup.steps[2].program, ".venv/bin/python");
    }
}
