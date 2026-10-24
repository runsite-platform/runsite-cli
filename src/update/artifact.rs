//! What a release ships for each platform. `runsite upgrade`, the installers
//! and the release workflow must agree on these names; a test reads all three.

pub const CHECKSUMS_FILE: &str = "SHA256SUMS.txt";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArchiveKind {
    TarGz,
    Zip,
}

impl ArchiveKind {
    fn extension(self) -> &'static str {
        match self {
            ArchiveKind::TarGz => "tar.gz",
            ArchiveKind::Zip => "zip",
        }
    }
}

/// One platform's build in a release.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub target: &'static str,
    pub archive: ArchiveKind,
}

pub const ALL: [Artifact; 5] = [
    Artifact::unix("x86_64-unknown-linux-musl"),
    Artifact::unix("aarch64-unknown-linux-musl"),
    Artifact::unix("x86_64-apple-darwin"),
    Artifact::unix("aarch64-apple-darwin"),
    Artifact {
        target: "x86_64-pc-windows-msvc",
        archive: ArchiveKind::Zip,
    },
];

fn archive_file(tag: &str, target: &str, kind: ArchiveKind) -> String {
    format!("runsite-{tag}-{target}.{}", kind.extension())
}

impl Artifact {
    const fn unix(target: &'static str) -> Self {
        Self {
            target,
            archive: ArchiveKind::TarGz,
        }
    }

    /// The build matching this binary, if releases ship one.
    pub fn current() -> Option<Self> {
        let target = if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            "x86_64-unknown-linux-musl"
        } else if cfg!(all(target_os = "linux", target_arch = "aarch64")) {
            "aarch64-unknown-linux-musl"
        } else if cfg!(all(target_os = "macos", target_arch = "x86_64")) {
            "x86_64-apple-darwin"
        } else if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            "aarch64-apple-darwin"
        } else if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            "x86_64-pc-windows-msvc"
        } else {
            return None;
        };
        ALL.into_iter().find(|artifact| artifact.target == target)
    }

    pub fn archive_name(self, tag: &str) -> String {
        archive_file(tag, self.target, self.archive)
    }

    /// The executable inside the archive.
    pub fn binary_name(self) -> &'static str {
        match self.archive {
            ArchiveKind::TarGz => "runsite",
            ArchiveKind::Zip => "runsite.exe",
        }
    }

    /// How to reinstall from scratch on this platform.
    pub fn reinstall_command(self) -> &'static str {
        match self.archive {
            ArchiveKind::TarGz => "curl -fsSL https://runsite.app/install.sh | sh",
            ArchiveKind::Zip => "irm https://runsite.app/install.ps1 | iex",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RELEASE_WORKFLOW: &str = include_str!("../../.github/workflows/release.yml");
    const INSTALL_SH: &str = include_str!("../../install/install.sh");
    const INSTALL_PS1: &str = include_str!("../../install/install.ps1");

    /// The lines of the workflow's build matrix entry for `target`.
    fn matrix_entry(target: &str) -> Vec<&'static str> {
        let lines: Vec<&str> = RELEASE_WORKFLOW.lines().collect();
        let start = lines
            .iter()
            .position(|line| line.trim() == format!("- target: {target}"))
            .unwrap_or_else(|| panic!("release.yml does not build {target}"));
        lines[start + 1..]
            .iter()
            .take_while(|line| !line.trim().starts_with("- target:") && !line.trim().is_empty())
            .copied()
            .collect()
    }

    #[test]
    fn the_release_workflow_builds_every_artifact_and_nothing_else() {
        let built = RELEASE_WORKFLOW
            .lines()
            .filter(|line| line.trim().starts_with("- target:"))
            .count();
        assert_eq!(built, ALL.len());
        for artifact in ALL {
            let entry = matrix_entry(artifact.target);
            let has = |setting: String| entry.iter().any(|line| line.trim() == setting);
            assert!(
                has(format!("binary: {}", artifact.binary_name())),
                "{}: binary name differs in release.yml",
                artifact.target
            );
            assert!(
                has(format!("archive_ext: {}", artifact.archive.extension())),
                "{}: archive type differs in release.yml",
                artifact.target
            );
        }
    }

    #[test]
    fn the_release_workflow_names_archives_and_checksums_alike() {
        let tag = "${{ github.ref_name }}";
        let target = "${{ matrix.target }}";
        for kind in [ArchiveKind::TarGz, ArchiveKind::Zip] {
            assert!(RELEASE_WORKFLOW.contains(&archive_file(tag, target, kind)));
        }
        assert!(RELEASE_WORKFLOW.contains(&format!("> {CHECKSUMS_FILE}")));
    }

    #[test]
    fn the_unix_installer_knows_every_unix_artifact() {
        for artifact in ALL
            .into_iter()
            .filter(|artifact| artifact.archive == ArchiveKind::TarGz)
        {
            let (arch, os) = artifact.target.split_once('-').unwrap();
            assert!(
                INSTALL_SH.contains(&format!("arch_triple=\"{arch}\"")),
                "{arch}"
            );
            assert!(INSTALL_SH.contains(&format!("os_triple=\"{os}\"")), "{os}");
        }
        assert!(INSTALL_SH.contains(&archive_file("${VERSION}", "${target}", ArchiveKind::TarGz)));
        assert!(INSTALL_SH.contains(&format!("${{base}}/{CHECKSUMS_FILE}")));
        assert!(INSTALL_SH.contains("\"$tmpdir/runsite\""));
    }

    #[test]
    fn the_windows_installer_knows_the_windows_artifact() {
        let windows = ALL
            .into_iter()
            .find(|artifact| artifact.archive == ArchiveKind::Zip)
            .unwrap();
        assert!(INSTALL_PS1.contains(&format!("$target = '{}'", windows.target)));
        assert!(INSTALL_PS1.contains(&archive_file("$Version", "$target", ArchiveKind::Zip)));
        assert!(INSTALL_PS1.contains(&format!("$base/{CHECKSUMS_FILE}")));
        assert!(INSTALL_PS1.contains(&format!("'{}'", windows.binary_name())));
    }

    #[test]
    fn archive_names_follow_the_release_layout() {
        let linux = Artifact::unix("x86_64-unknown-linux-musl");
        assert_eq!(
            linux.archive_name("v0.4.0"),
            "runsite-v0.4.0-x86_64-unknown-linux-musl.tar.gz"
        );
        assert_eq!(
            ALL[4].archive_name("v0.4.0"),
            "runsite-v0.4.0-x86_64-pc-windows-msvc.zip"
        );
    }

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    #[test]
    fn this_platform_has_a_release_build() {
        assert_eq!(
            Artifact::current().map(|artifact| artifact.target),
            Some("x86_64-unknown-linux-musl")
        );
    }
}
