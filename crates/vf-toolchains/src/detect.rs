//! Detection by layers, from exact to rough: hints in `AGENTS.md` → version files
//! (`.java-version`, `.tool-versions`, `.nvmrc`, `.node-version`, `.python-version`) → heuristics by
//! build files. A later layer only fills in tools an earlier one did not name. Asking a model is
//! the next layer (not yet).

use std::path::Path;

/// A tool a project needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Need {
    /// Tool: `java`, `gradle`, `maven`, `node`, `python`.
    pub name: String,
    /// `mise` spec, e.g. `java@temurin-21`.
    pub spec: String,
}

/// Default Java when a JVM project does not say.
pub const DEFAULT_JAVA: &str = "21";

fn need(name: &str, version: &str) -> Need {
    let spec = match name {
        "java" => format!(
            "java@temurin-{}",
            version
                .trim_start_matches("temurin-")
                .split('.')
                .next()
                .unwrap_or(DEFAULT_JAVA)
        ),
        _ => format!("{name}@{}", version.trim_start_matches('v')),
    };
    Need {
        name: name.into(),
        spec,
    }
}

/// What `dir` needs.
pub fn detect(dir: &Path) -> Vec<Need> {
    let has = |f: &str| dir.join(f).exists();
    let read = |f: &str| {
        std::fs::read_to_string(dir.join(f))
            .map(|s| s.trim().to_owned())
            .unwrap_or_default()
    };
    let mut out: Vec<Need> = Vec::new();
    let add = |n: Need, out: &mut Vec<Need>| {
        if !out.iter().any(|x| x.name == n.name) {
            out.push(n);
        }
    };

    // 1. Hints of the maintainer in AGENTS.md: "Java 17", "JDK 17", "Node 20", "Node.js 22", "Python 3.12".
    for (name, version) in agents_hints(&read("AGENTS.md")) {
        add(need(&name, &version), &mut out);
    }

    // 2. Version files.
    if !read(".java-version").is_empty() {
        add(need("java", &read(".java-version")), &mut out);
    }
    for line in read(".tool-versions").lines() {
        let mut parts = line.split_whitespace();
        if let (Some(tool), Some(version)) = (parts.next(), parts.next())
            && ["java", "node", "python", "gradle", "maven"].contains(&tool)
        {
            add(need(tool, version), &mut out);
        }
    }
    for f in [".nvmrc", ".node-version"] {
        let v = read(f);
        if !v.is_empty()
            && v.chars()
                .next()
                .is_some_and(|c| c.is_ascii_digit() || c == 'v')
        {
            add(need("node", &v), &mut out);
        }
    }
    if !read(".python-version").is_empty() {
        add(need("python", &read(".python-version")), &mut out);
    }

    // 3. Heuristics by build files.
    let gradle = has("build.gradle")
        || has("build.gradle.kts")
        || has("settings.gradle")
        || has("settings.gradle.kts");
    if has("gradlew") || gradle || has("pom.xml") {
        add(need("java", DEFAULT_JAVA), &mut out);
        if gradle && !has("gradlew") {
            add(need("gradle", "8"), &mut out);
        }
        if has("pom.xml") && !has("mvnw") {
            add(need("maven", "3"), &mut out);
        }
    }
    out
}

/// `(tool, version)` pairs mentioned in `AGENTS.md`.
fn agents_hints(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let words: Vec<String> = text
        .split(|c: char| c.is_whitespace() || matches!(c, ',' | ';' | ':' | '(' | ')' | '`' | '*'))
        .filter(|w| !w.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    for pair in words.windows(2) {
        let tool = match pair[0].as_str() {
            "java" | "jdk" => "java",
            "node" | "node.js" | "nodejs" => "node",
            "python" => "python",
            _ => continue,
        };
        let version = pair[1].trim_start_matches('v').trim_end_matches(['.', '+']);
        let valid = !version.is_empty()
            && version.chars().all(|c| c.is_ascii_digit() || c == '.')
            && version.chars().next().is_some_and(|c| c.is_ascii_digit());
        if valid && !out.iter().any(|(t, _): &(String, String)| t == tool) {
            out.push((tool.to_owned(), version.to_owned()));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project(files: &[(&str, &str)]) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or_default();
        let dir = std::env::temp_dir().join(format!("vf-detect-{}-{nanos}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        for (f, c) in files {
            let _ = std::fs::write(dir.join(f), c);
        }
        dir
    }

    fn specs(files: &[(&str, &str)]) -> Vec<String> {
        let dir = project(files);
        let out = detect(&dir).into_iter().map(|n| n.spec).collect();
        let _ = std::fs::remove_dir_all(&dir);
        out
    }

    #[test]
    fn layers() {
        assert!(specs(&[("index.js", "")]).is_empty());
        assert_eq!(
            specs(&[("gradlew", ""), ("build.gradle", "")]),
            ["java@temurin-21"]
        );
        assert_eq!(
            specs(&[("build.gradle.kts", "")]),
            ["java@temurin-21", "gradle@8"]
        );
        assert_eq!(
            specs(&[("pom.xml", ""), (".java-version", "17.0.2")]),
            ["java@temurin-17", "maven@3"]
        );
        assert_eq!(
            specs(&[
                ("gradlew", ""),
                ("AGENTS.md", "Build needs **JDK 11** and Node.js v20.")
            ]),
            ["java@temurin-11", "node@20"]
        );
        assert_eq!(
            specs(&[
                (".tool-versions", "java temurin-17.0.1\nnode 22.1.0\nruby 3"),
                ("gradlew", "")
            ]),
            ["java@temurin-17", "node@22.1.0"]
        );
        assert_eq!(
            specs(&[("package.json", "{}"), (".nvmrc", "lts/*")]),
            Vec::<String>::new(),
            "aliases are not versions"
        );
        assert_eq!(
            specs(&[("AGENTS.md", "Use java 17"), (".java-version", "21")]),
            ["java@temurin-17"],
            "AGENTS.md first"
        );
    }
}
