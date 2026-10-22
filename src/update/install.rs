//! `runsite upgrade`: download the latest release, verify it and replace this binary.

use super::artifact::{ArchiveKind, Artifact, CHECKSUMS_FILE};
use super::notice::update_available;
use super::{
    download, http_client, is_newer, notice, version_of, Release, ReleaseSource, CURRENT_VERSION,
};
use crate::output::Report;
use anyhow::{Context, Result};
use colored::Colorize;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

const MAX_ARCHIVE_BYTES: usize = 200 * 1024 * 1024;
const MAX_CHECKSUMS_BYTES: usize = 1024 * 1024;

/// What `runsite upgrade` reports: the versions for `--check`, else the outcome.
#[derive(Debug, PartialEq, Serialize)]
#[serde(untagged)]
pub enum UpgradeReport {
    Status {
        current_version: String,
        latest_version: String,
        update_available: bool,
    },
    Outcome {
        previous_version: String,
        current_version: String,
        updated: bool,
    },
}

impl Report for UpgradeReport {
    fn write_text(&self, out: &mut dyn Write) -> io::Result<()> {
        match self {
            UpgradeReport::Status {
                current_version,
                latest_version,
                update_available: true,
            } => writeln!(out, "{}", update_available(current_version, latest_version)),
            UpgradeReport::Status {
                current_version, ..
            } => writeln!(out, "runsite v{current_version} is up to date"),
            UpgradeReport::Outcome {
                previous_version,
                current_version,
                updated: true,
            } => writeln!(
                out,
                "{} runsite v{previous_version} → v{current_version}",
                "Updated".green()
            ),
            UpgradeReport::Outcome {
                current_version, ..
            } => writeln!(
                out,
                "runsite v{current_version} is already the latest version"
            ),
        }
    }
}

/// What to do once the latest release is known.
#[derive(Debug, PartialEq)]
enum Decision {
    Report(UpgradeReport),
    Install,
}

fn decide(latest_tag: &str, current: &str, check_only: bool) -> Decision {
    let latest_version = version_of(latest_tag).to_string();
    let update_available = is_newer(latest_tag, current);
    if check_only {
        return Decision::Report(UpgradeReport::Status {
            current_version: current.to_string(),
            latest_version,
            update_available,
        });
    }
    if !update_available {
        return Decision::Report(UpgradeReport::Outcome {
            previous_version: current.to_string(),
            current_version: current.to_string(),
            updated: false,
        });
    }
    Decision::Install
}

pub async fn upgrade(check_only: bool) -> Result<UpgradeReport> {
    let client = http_client(Duration::from_secs(120))?;
    let release = ReleaseSource::github().latest(&client).await?;
    notice::record_check(Some(&release.tag_name));
    match decide(&release.tag_name, CURRENT_VERSION, check_only) {
        Decision::Report(report) => Ok(report),
        Decision::Install => install(&client, &release).await,
    }
}

/// Replace the running binary with `release`, after checking it is ours to
/// replace and that the new one is intact and runs.
async fn install(client: &reqwest::Client, release: &Release) -> Result<UpgradeReport> {
    let current_exe = std::env::current_exe()
        .and_then(|path| path.canonicalize())
        .context("cannot locate the running runsite binary")?;
    refuse_unmanaged_install(&current_exe, cargo_home().as_deref())?;

    let artifact = Artifact::current().context(
        "there is no prebuilt runsite binary for this platform; build it from source instead",
    )?;
    let latest_version = version_of(&release.tag_name).to_string();

    eprintln!(
        "Downloading runsite {} for {}…",
        release.tag_name, artifact.target
    );
    let staged = prepare_binary(client, release, artifact, &latest_version).await?;

    self_replace::self_replace(staged.path()).map_err(|error| {
        if error.kind() == std::io::ErrorKind::PermissionDenied {
            anyhow::anyhow!(
                "no permission to replace {}. Re-run with the rights to write there, or reinstall:\n  {}",
                current_exe.display(),
                artifact.reinstall_command()
            )
        } else {
            anyhow::Error::new(error)
                .context(format!("failed to replace {}", current_exe.display()))
        }
    })?;

    Ok(UpgradeReport::Outcome {
        previous_version: CURRENT_VERSION.to_string(),
        current_version: latest_version,
        updated: true,
    })
}

/// A downloaded, verified binary that ran once, waiting in a temporary
/// directory to replace the current one. The directory goes when this drops.
struct StagedBinary {
    _directory: tempfile::TempDir,
    path: PathBuf,
}

impl StagedBinary {
    fn path(&self) -> &Path {
        &self.path
    }
}

/// Everything before the swap: download, verify the checksum, unpack, and run
/// the new binary once so a broken build never replaces a working one.
async fn prepare_binary(
    client: &reqwest::Client,
    release: &Release,
    artifact: Artifact,
    expected_version: &str,
) -> Result<StagedBinary> {
    let binary = download_verified_binary(client, release, artifact).await?;
    let directory = tempfile::Builder::new()
        .prefix("runsite-upgrade")
        .tempdir()
        .context("cannot create a temporary directory")?;
    let path = directory.path().join(artifact.binary_name());
    write_executable(&path, &binary)?;
    check_binary_runs(&path, expected_version).await?;
    Ok(StagedBinary {
        _directory: directory,
        path,
    })
}

fn cargo_home() -> Option<PathBuf> {
    std::env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|home| home.join(".cargo")))
}

/// A binary installed by cargo or running from a build directory is not ours to overwrite.
fn refuse_unmanaged_install(current_exe: &Path, cargo_home: Option<&Path>) -> Result<()> {
    if is_cargo_install(current_exe, cargo_home) {
        anyhow::bail!(
            "runsite was installed with cargo; update it with cargo instead of `runsite upgrade`"
        );
    }
    let in_build_dir = current_exe
        .ancestors()
        .any(|dir| dir.file_name().is_some_and(|name| name == "target"));
    if in_build_dir {
        anyhow::bail!(
            "this is a development build ({}); rebuild it instead of upgrading",
            current_exe.display()
        );
    }
    Ok(())
}

fn is_cargo_install(current_exe: &Path, cargo_home: Option<&Path>) -> bool {
    let Some(cargo_bin) = cargo_home.map(|home| home.join("bin")) else {
        return false;
    };
    let cargo_bin = cargo_bin.canonicalize().unwrap_or(cargo_bin);
    current_exe.parent() == Some(cargo_bin.as_path())
}

/// Download `artifact`'s archive, check it against the release checksums and unpack the binary.
async fn download_verified_binary(
    client: &reqwest::Client,
    release: &Release,
    artifact: Artifact,
) -> Result<Vec<u8>> {
    let target = artifact.target;
    let archive = artifact.archive_name(&release.tag_name);
    let archive_asset = release.asset(&archive).with_context(|| {
        format!(
            "release {} has no build for {target} ({archive})",
            release.tag_name
        )
    })?;
    let checksums_asset = release.asset(CHECKSUMS_FILE).with_context(|| {
        format!(
            "release {} has no {CHECKSUMS_FILE}; refusing to install an unverified binary",
            release.tag_name
        )
    })?;

    let checksums = download(
        client,
        &checksums_asset.browser_download_url,
        MAX_CHECKSUMS_BYTES,
    )
    .await?;
    let checksums = String::from_utf8_lossy(&checksums);
    let expected = expected_checksum(&checksums, &archive)
        .with_context(|| format!("no checksum listed for {archive}"))?;

    let archive_bytes = download(
        client,
        &archive_asset.browser_download_url,
        MAX_ARCHIVE_BYTES,
    )
    .await?;
    let actual = sha256_hex(&archive_bytes);
    if !actual.eq_ignore_ascii_case(&expected) {
        anyhow::bail!(
            "checksum mismatch for {archive}\n  expected: {expected}\n  actual:   {actual}"
        );
    }

    match artifact.archive {
        ArchiveKind::Zip => extract_from_zip(&archive_bytes, artifact.binary_name()),
        ArchiveKind::TarGz => extract_from_tar_gz(&archive_bytes, artifact.binary_name()),
    }
}

/// Find `file_name` in `sha256sum` output (`<hash>  <name>` or `<hash> *<name>`).
fn expected_checksum(checksums: &str, file_name: &str) -> Option<String> {
    checksums.lines().find_map(|line| {
        let (hash, name) = line.trim().split_once(char::is_whitespace)?;
        let name = name.trim_start().trim_start_matches('*');
        (name == file_name).then(|| hash.to_string())
    })
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn extract_from_tar_gz(archive_bytes: &[u8], binary_name: &str) -> Result<Vec<u8>> {
    let decoder = flate2::read::GzDecoder::new(archive_bytes);
    let mut archive = tar::Archive::new(decoder);
    for entry in archive
        .entries()
        .context("cannot read the release archive")?
    {
        let entry = entry.context("cannot read the release archive")?;
        let is_binary = entry.header().entry_type().is_file()
            && entry
                .path()
                .ok()
                .is_some_and(|path| path.file_name().is_some_and(|name| name == binary_name));
        if is_binary {
            return read_limited(entry);
        }
    }
    anyhow::bail!("{binary_name} not found in the release archive")
}

fn extract_from_zip(archive_bytes: &[u8], binary_name: &str) -> Result<Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(archive_bytes))
        .context("cannot read the release archive")?;
    let entry = archive
        .by_name(binary_name)
        .with_context(|| format!("{binary_name} not found in the release archive"))?;
    read_limited(entry)
}

fn read_limited(reader: impl Read) -> Result<Vec<u8>> {
    let mut contents = Vec::new();
    reader
        .take(MAX_ARCHIVE_BYTES as u64 + 1)
        .read_to_end(&mut contents)
        .context("cannot unpack the release archive")?;
    if contents.len() > MAX_ARCHIVE_BYTES {
        anyhow::bail!("the binary in the release archive is larger than expected");
    }
    Ok(contents)
}

fn write_executable(path: &Path, contents: &[u8]) -> Result<()> {
    std::fs::write(path, contents)
        .with_context(|| format!("failed to write {}", path.display()))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .with_context(|| format!("failed to make {} executable", path.display()))?;
    }
    Ok(())
}

/// Run the downloaded binary once, so a broken build never replaces a working one.
async fn check_binary_runs(path: &Path, expected_version: &str) -> Result<()> {
    let run = tokio::process::Command::new(path)
        .arg("--version")
        .kill_on_drop(true)
        .output();
    let result = tokio::time::timeout(Duration::from_secs(15), run)
        .await
        .context("the downloaded binary did not respond; keeping the current version")?
        .context(
            "the downloaded binary does not run on this system; keeping the current version",
        )?;
    let reported = String::from_utf8_lossy(&result.stdout);
    // `runsite --version` prints `runsite 0.4.0`; a substring match would take 10.4.0 too.
    let reports_expected = reported
        .split_whitespace()
        .any(|word| version_of(word) == expected_version);
    if !result.status.success() || !reports_expected {
        anyhow::bail!(
            "the downloaded binary reports `{}` instead of v{expected_version}; keeping the current version",
            reported.trim()
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::update::ReleaseAsset;
    use std::io::Write;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const TARGET: Artifact = crate::update::artifact::ALL[0];

    fn tar_gz_with(file_name: &str, contents: &[u8]) -> Vec<u8> {
        let encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_size(contents.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, file_name, contents)
            .unwrap();
        builder.into_inner().unwrap().finish().unwrap()
    }

    async fn serve_release(archive_bytes: Vec<u8>, checksums: String) -> (MockServer, Release) {
        let server = MockServer::start().await;
        let archive = TARGET.archive_name("v9.9.9");
        Mock::given(method("GET"))
            .and(path(format!("/{archive}")))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(archive_bytes))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/SHA256SUMS.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_string(checksums))
            .mount(&server)
            .await;
        let release = Release {
            tag_name: "v9.9.9".to_string(),
            assets: vec![
                ReleaseAsset {
                    browser_download_url: format!("{}/{archive}", server.uri()),
                    name: archive,
                },
                ReleaseAsset {
                    name: CHECKSUMS_FILE.to_string(),
                    browser_download_url: format!("{}/SHA256SUMS.txt", server.uri()),
                },
            ],
        };
        (server, release)
    }

    #[tokio::test]
    async fn installs_only_a_binary_whose_checksum_matches() {
        let archive_bytes = tar_gz_with("runsite", b"new binary");
        let checksums = format!(
            "{}  {}\n",
            sha256_hex(&archive_bytes),
            TARGET.archive_name("v9.9.9")
        );
        let (_server, release) = serve_release(archive_bytes, checksums).await;

        let client = http_client(Duration::from_secs(5)).unwrap();
        let binary = download_verified_binary(&client, &release, TARGET)
            .await
            .unwrap();
        assert_eq!(binary, b"new binary");
    }

    #[tokio::test]
    async fn rejects_a_tampered_archive() {
        let archive_bytes = tar_gz_with("runsite", b"new binary");
        let checksums = format!(
            "{}  {}\n",
            sha256_hex(b"something else"),
            TARGET.archive_name("v9.9.9")
        );
        let (_server, release) = serve_release(archive_bytes, checksums).await;

        let client = http_client(Duration::from_secs(5)).unwrap();
        let error = download_verified_binary(&client, &release, TARGET)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("checksum mismatch"));
    }

    #[tokio::test]
    async fn rejects_a_release_without_this_platform() {
        let (_server, release) = serve_release(Vec::new(), String::new()).await;
        let client = http_client(Duration::from_secs(5)).unwrap();
        let error = download_verified_binary(&client, &release, crate::update::artifact::ALL[3])
            .await
            .unwrap_err();
        assert!(error
            .to_string()
            .contains("no build for aarch64-apple-darwin"));
    }

    #[test]
    fn reads_both_sha256sum_formats() {
        let checksums = "aaa  runsite-v1.0.0-x.tar.gz\nbbb *runsite-v1.0.0-y.zip\n";
        assert_eq!(
            expected_checksum(checksums, "runsite-v1.0.0-x.tar.gz").as_deref(),
            Some("aaa")
        );
        assert_eq!(
            expected_checksum(checksums, "runsite-v1.0.0-y.zip").as_deref(),
            Some("bbb")
        );
        assert_eq!(expected_checksum(checksums, "runsite-v1.0.0-z.zip"), None);
    }

    #[test]
    fn unpacks_the_windows_zip() {
        let mut buffer = Cursor::new(Vec::new());
        {
            let mut writer = zip::ZipWriter::new(&mut buffer);
            writer
                .start_file("runsite.exe", zip::write::SimpleFileOptions::default())
                .unwrap();
            writer.write_all(b"windows binary").unwrap();
            writer.finish().unwrap();
        }
        let binary = extract_from_zip(buffer.get_ref(), "runsite.exe").unwrap();
        assert_eq!(binary, b"windows binary");
    }

    #[test]
    fn a_tarball_without_the_binary_is_an_error() {
        let archive_bytes = tar_gz_with("README.md", b"docs");
        assert!(extract_from_tar_gz(&archive_bytes, "runsite").is_err());
    }

    #[test]
    fn development_builds_are_not_upgraded() {
        let error =
            refuse_unmanaged_install(Path::new("/src/runsite-cli/target/release/runsite"), None)
                .unwrap_err();
        assert!(error.to_string().contains("development build"));
        assert!(refuse_unmanaged_install(Path::new("/opt/tools/runsite"), None).is_ok());
    }

    #[test]
    fn a_cargo_install_is_left_to_cargo() {
        let cargo_home = tempfile::tempdir().unwrap();
        std::fs::create_dir(cargo_home.path().join("bin")).unwrap();
        let installed = cargo_home
            .path()
            .join("bin")
            .canonicalize()
            .unwrap()
            .join("runsite");

        let error = refuse_unmanaged_install(&installed, Some(cargo_home.path())).unwrap_err();
        assert!(error.to_string().contains("installed with cargo"));
    }

    fn release_of_script(version: &str) -> (Vec<u8>, String) {
        let script = format!("#!/bin/sh\necho \"runsite {version}\"\n");
        let archive_bytes = tar_gz_with("runsite", script.as_bytes());
        let checksums = format!(
            "{}  {}\n",
            sha256_hex(&archive_bytes),
            TARGET.archive_name("v9.9.9")
        );
        (archive_bytes, checksums)
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_verified_binary_that_reports_its_version_is_staged() {
        let (archive_bytes, checksums) = release_of_script("9.9.9");
        let (_server, release) = serve_release(archive_bytes, checksums).await;
        let client = http_client(Duration::from_secs(5)).unwrap();

        let staged = prepare_binary(&client, &release, TARGET, "9.9.9")
            .await
            .unwrap();
        assert!(staged.path().is_file());
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_binary_whose_version_only_contains_the_expected_one_is_not_staged() {
        let (archive_bytes, checksums) = release_of_script("19.9.9");
        let (_server, release) = serve_release(archive_bytes, checksums).await;
        let client = http_client(Duration::from_secs(5)).unwrap();

        let error = prepare_binary(&client, &release, TARGET, "9.9.9")
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("instead of v9.9.9"), "{error}");
    }

    #[test]
    fn check_only_reports_both_versions() {
        let decision = decide("v9.9.9", "0.3.0", true);
        let Decision::Report(report) = decision else {
            panic!("expected a report");
        };
        assert_eq!(
            crate::output::render_json(&report),
            serde_json::json!({
                "current_version": "0.3.0", "latest_version": "9.9.9", "update_available": true
            })
        );
        assert!(crate::output::render_text(&report).contains("v0.3.0 → v9.9.9"));
    }

    #[test]
    fn an_up_to_date_binary_is_left_alone() {
        let Decision::Report(report) = decide("v0.3.0", "0.3.0", false) else {
            panic!("expected a report");
        };
        assert_eq!(
            crate::output::render_text(&report),
            "runsite v0.3.0 is already the latest version\n"
        );
    }

    #[test]
    fn a_newer_release_is_installed() {
        assert_eq!(decide("v0.4.0", "0.3.0", false), Decision::Install);
    }

    #[test]
    fn the_reinstall_hint_matches_the_platform() {
        assert!(TARGET.reinstall_command().contains("install.sh"));
        assert!(crate::update::artifact::ALL[4]
            .reinstall_command()
            .contains("install.ps1"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn a_binary_reporting_another_version_is_not_staged() {
        let (archive_bytes, checksums) = release_of_script("1.0.0");
        let (_server, release) = serve_release(archive_bytes, checksums).await;
        let client = http_client(Duration::from_secs(5)).unwrap();

        let error = prepare_binary(&client, &release, TARGET, "9.9.9")
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("instead of v9.9.9"), "{error}");
    }
}
