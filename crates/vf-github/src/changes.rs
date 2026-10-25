//! What code from outside the original (a fork or a pull request) changes, by file name. The names
//! come from GitHub and are untrusted: they are only compared, never used as paths.

/// A git ref to take a project at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitRef {
    /// The head of pull request `N` of the repository.
    Pull(u64),
    /// A branch, tag or commit.
    Named(String),
}

impl GitRef {
    /// Parses `pull/<N>/head` or a branch / tag / commit name. `None` for an empty or unsafe value
    /// (spaces, `..`, a leading `-` or `/`, characters outside a git ref name).
    pub fn parse(input: &str) -> Option<Self> {
        let s = input.trim();
        if let Some(n) = s
            .strip_prefix("pull/")
            .and_then(|r| r.strip_suffix("/head"))
        {
            return n.parse().ok().filter(|n| *n > 0).map(Self::Pull);
        }
        let ok = !s.is_empty()
            && s.len() <= 200
            && !s.starts_with(['-', '/'])
            && !s.contains("..")
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/'));
        ok.then(|| Self::Named(s.to_owned()))
    }

    /// The ref as written in a URL: `pull/<N>/head` or the name.
    pub fn as_str(&self) -> String {
        match self {
            Self::Pull(n) => format!("pull/{n}/head"),
            Self::Named(s) => s.clone(),
        }
    }
}

/// Changed files of foreign code, sorted into the kinds a person should look at before building.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangedFiles {
    /// All changed paths (as listed by GitHub).
    pub all: Vec<String>,
    /// Whether GitHub listed every changed file (it stops at a limit).
    pub complete: bool,
}

/// Dependency manifests and lockfiles: a change can pull new code from a registry.
pub fn is_dependency_file(path: &str) -> bool {
    const NAMES: &[&str] = &[
        "package.json",
        "package-lock.json",
        "pnpm-lock.yaml",
        "yarn.lock",
        "bun.lockb",
        "Cargo.toml",
        "Cargo.lock",
        "go.mod",
        "go.sum",
        "pyproject.toml",
        "poetry.lock",
        "Pipfile",
        "Pipfile.lock",
        "setup.cfg",
        "build.gradle",
        "build.gradle.kts",
        "settings.gradle",
        "settings.gradle.kts",
        "gradle.properties",
        "pom.xml",
        "Gemfile",
        "Gemfile.lock",
        "composer.json",
        "composer.lock",
        "Package.swift",
        "Podfile",
        "pubspec.yaml",
        "mix.exs",
        "deno.json",
        "vcpkg.json",
        "conanfile.txt",
    ];
    let name = file_name(path);
    NAMES.contains(&name)
        || (name.starts_with("requirements") && name.ends_with(".txt"))
        || name.ends_with(".csproj")
        || name.ends_with(".fsproj")
        || name.ends_with(".gemspec")
        || name.ends_with(".cabal")
        || name.ends_with(".nuspec")
}

/// Build scripts, installers and CI: a change runs on the machine during the build.
pub fn is_build_script(path: &str) -> bool {
    const NAMES: &[&str] = &[
        "Makefile",
        "makefile",
        "GNUmakefile",
        "CMakeLists.txt",
        "build.rs",
        "setup.py",
        "meson.build",
        "Dockerfile",
        "Containerfile",
        "Justfile",
        "justfile",
        "Taskfile.yml",
        "build.xml",
        "gradlew",
        "gradlew.bat",
        "mvnw",
        "mvnw.cmd",
        "configure",
        "configure.ac",
        "SConstruct",
        "Rakefile",
        "vite.config.ts",
        "vite.config.js",
        "webpack.config.js",
    ];
    const EXTS: &[&str] = &[
        ".sh", ".bash", ".ps1", ".psm1", ".bat", ".cmd", ".cmake", ".mk",
    ];
    let name = file_name(path);
    NAMES.contains(&name)
        || EXTS.iter().any(|e| name.ends_with(e))
        || path.starts_with(".github/workflows/")
        || path.starts_with(".gitlab-ci")
        || path.contains("gradle/wrapper/")
}

/// Executables and other binary files: nobody can review them by reading.
pub fn is_binary(path: &str) -> bool {
    const EXTS: &[&str] = &[
        ".exe", ".dll", ".so", ".dylib", ".a", ".lib", ".o", ".obj", ".bin", ".jar", ".war",
        ".class", ".pyc", ".whl", ".node", ".wasm", ".msi", ".apk", ".dmg", ".pkg", ".deb", ".rpm",
        ".zip", ".7z", ".rar", ".tar", ".gz", ".tgz", ".xz",
    ];
    let lower = file_name(path).to_ascii_lowercase();
    EXTS.iter().any(|e| lower.ends_with(e))
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refs() {
        assert_eq!(GitRef::parse("pull/42/head"), Some(GitRef::Pull(42)));
        assert_eq!(GitRef::parse("pull/0/head"), None);
        assert_eq!(GitRef::parse("pull/x/head"), None);
        assert_eq!(
            GitRef::parse("feature/fix-1.2"),
            Some(GitRef::Named("feature/fix-1.2".into()))
        );
        assert_eq!(
            GitRef::parse("v1.0.0").map(|r| r.as_str()),
            Some("v1.0.0".into())
        );
        for bad in ["", " ", "-x", "/x", "a..b", "a b", "a;b", "$(x)"] {
            assert_eq!(GitRef::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn kinds_of_files() {
        assert!(is_dependency_file("package.json"));
        assert!(is_dependency_file("app/build.gradle.kts"));
        assert!(is_dependency_file("requirements-dev.txt"));
        assert!(is_dependency_file("src/App.csproj"));
        assert!(!is_dependency_file("src/package.rs"));
        assert!(is_build_script(".github/workflows/ci.yml"));
        assert!(is_build_script("scripts/install.sh"));
        assert!(is_build_script("crates/x/build.rs"));
        assert!(is_build_script("gradle/wrapper/gradle-wrapper.properties"));
        assert!(!is_build_script("src/main.rs"));
        assert!(is_binary("tools/helper.EXE"));
        assert!(is_binary("lib/native.so"));
        assert!(!is_binary("README.md"));
    }
}
