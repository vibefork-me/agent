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
        }
    }

    /// Whether the output runs on any OS (JVM jars, Node scripts).
    pub fn portable(self) -> bool {
        matches!(self, Self::Gradle | Self::Maven | Self::Node)
    }
}

/// How to build a project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recipe {
    /// Ecosystem.
    pub ecosystem: Ecosystem,
    /// Program (relative to the project for wrappers).
    pub program: String,
    /// Arguments.
    pub args: Vec<String>,
    /// Folders (relative) holding the outputs.
    pub outputs: Vec<String>,
}

/// The recipe of `dir`, if the project type is known.
pub fn detect_recipe(dir: &Path) -> Option<Recipe> {
    let has = |f: &str| dir.join(f).exists();
    let r = |ecosystem, program: &str, args: &[&str], outputs: &[&str]| Recipe {
        ecosystem,
        program: program.to_owned(),
        args: args.iter().map(|s| (*s).to_owned()).collect(),
        outputs: outputs.iter().map(|s| (*s).to_owned()).collect(),
    };
    let wrapper = |name: &str| {
        if cfg!(windows) {
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
            return Some(r(
                Ecosystem::Node,
                "npm",
                &["run", "build"],
                &["dist", "build"],
            ));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn with(files: &[(&str, &str)]) -> Option<Recipe> {
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
            let _ = std::fs::write(dir.join(f), c);
        }
        let r = detect_recipe(&dir);
        let _ = std::fs::remove_dir_all(&dir);
        r
    }

    #[test]
    fn recipes() {
        assert_eq!(
            with(&[("gradlew", ""), ("build.gradle", "")]).map(|r| r.ecosystem),
            Some(Ecosystem::Gradle)
        );
        assert_eq!(
            with(&[("pom.xml", "")]).map(|r| r.program),
            Some("mvn".into())
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
}
