//! # warrant-verify runner
//!
//! Abstraction and implementations for executing safe Rust harnesses under Miri,
//! supporting both direct host cargo-miri execution and hardened Docker sandbox containers.
//!
//! Strictly enforces SPEC.md Section 5.2 (Miri invocation) and Section 6 (Sandbox requirements).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tokio::time::timeout;
use crate::VerifyError;

/// Output captured from a Miri test execution
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MiriOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
}

/// Abstract runner for executing Miri verifications
#[async_trait]
pub trait ProofRunner: Send + Sync {
    /// Execute `cargo miri test --lib <filter>` in the target package directory
    async fn run_miri(&self, package_dir: &Path, filter: &str) -> Result<MiriOutput, VerifyError>;

    /// Check compilation with `cargo test --no-run` (SPEC.md Section 5.2 Step 2)
    async fn check_compilation(&self, package_dir: &Path) -> Result<MiriOutput, VerifyError> {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(package_dir)
            .arg("test")
            .arg("--no-run")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = cmd.spawn()?;
        let output = child.wait_with_output().await?;
        Ok(MiriOutput {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: 0,
        })
    }

    /// Execute test suite natively without Miri: `cargo test --lib` (SPEC.md Section 5.3 Step 3)
    async fn run_native_test(&self, package_dir: &Path) -> Result<MiriOutput, VerifyError> {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(package_dir)
            .arg("test")
            .arg("--lib")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let child = cmd.spawn()?;
        let output = child.wait_with_output().await?;
        Ok(MiriOutput {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: 0,
        })
    }
}

/// Local host runner executing `cargo miri test`
#[derive(Debug, Clone)]
pub struct LocalMiriRunner {
    pub timeout_duration: Duration,
    pub miri_flags: Vec<String>,
    pub shared_target_dir: Option< std::path::PathBuf >,
}

impl Default for LocalMiriRunner {
    fn default() -> Self {
        Self {
            timeout_duration: Duration::from_secs(60),
            miri_flags: vec![
                "-Zmiri-tree-borrows".to_string(),
                "-Zmiri-strict-provenance".to_string(),
                "-Zmiri-preemption-rate=0".to_string(),
            ],
            shared_target_dir: None,
        }
    }
}

impl LocalMiriRunner {
    pub fn new(timeout_duration: Duration, miri_flags: Vec<String>) -> Self {
        Self {
            timeout_duration,
            miri_flags,
            shared_target_dir: None,
        }
    }

    pub fn with_shared_target_dir(mut self, target_dir: std::path::PathBuf) -> Self {
        self.shared_target_dir = Some(target_dir);
        self
    }
}

#[async_trait]
impl ProofRunner for LocalMiriRunner {
    async fn run_miri(&self, package_dir: &Path, filter: &str) -> Result<MiriOutput, VerifyError> {
        let start_time = Instant::now();

        let mut cmd = Command::new("cargo");
        cmd.current_dir(package_dir)
            .arg("miri")
            .arg("test")
            .arg("--lib")
            .arg(filter)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        cmd.env("RUSTUP_TOOLCHAIN", crate::bundle::PINNED_TOOLCHAIN);

        if !self.miri_flags.is_empty() {
            cmd.env("MIRIFLAGS", self.miri_flags.join(" "));
        }
        if let Some(ref target_dir) = self.shared_target_dir {
            cmd.env("CARGO_TARGET_DIR", target_dir);
        }

        let child = cmd.spawn()?;
        match timeout(self.timeout_duration, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                Ok(MiriOutput {
                    exit_code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    duration_ms,
                })
            }
            Ok(Err(e)) => Err(VerifyError::Io(e)),
            Err(_) => Err(VerifyError::Timeout(self.timeout_duration)),
        }
    }

    async fn check_compilation(&self, package_dir: &Path) -> Result<MiriOutput, VerifyError> {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(package_dir)
            .arg("test")
            .arg("--no-run")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.env("RUSTUP_TOOLCHAIN", crate::bundle::PINNED_TOOLCHAIN);
        if let Some(ref target_dir) = self.shared_target_dir {
            cmd.env("CARGO_TARGET_DIR", target_dir);
        }
        let child = cmd.spawn()?;
        let output = child.wait_with_output().await?;
        Ok(MiriOutput {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: 0,
        })
    }

    async fn run_native_test(&self, package_dir: &Path) -> Result<MiriOutput, VerifyError> {
        let mut cmd = Command::new("cargo");
        cmd.current_dir(package_dir)
            .arg("test")
            .arg("--lib")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        cmd.env("RUSTUP_TOOLCHAIN", crate::bundle::PINNED_TOOLCHAIN);
        if let Some(ref target_dir) = self.shared_target_dir {
            cmd.env("CARGO_TARGET_DIR", target_dir);
        }
        let child = cmd.spawn()?;
        let output = child.wait_with_output().await?;
        Ok(MiriOutput {
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            duration_ms: 0,
        })
    }
}

/// Hardened Docker sandbox runner
/// Strictly enforces: --network none --read-only --tmpfs /tmp:rw,noexec,nosuid --pids-limit 64 --memory 2g
#[derive(Debug, Clone)]
pub struct DockerMiriRunner {
    pub image_name: String,
    pub timeout_duration: Duration,
    pub miri_flags: Vec<String>,
}

impl Default for DockerMiriRunner {
    fn default() -> Self {
        let image = std::env::var("WARRANT_DOCKER_IMAGE")
            .unwrap_or_else(|_| "warrant-verifier:latest".to_string());
        Self {
            image_name: image,
            timeout_duration: Duration::from_secs(60),
            miri_flags: vec![
                "-Zmiri-tree-borrows".to_string(),
                "-Zmiri-strict-provenance".to_string(),
                "-Zmiri-preemption-rate=0".to_string(),
            ],
        }
    }
}

impl DockerMiriRunner {
    pub fn new(image_name: String, timeout_duration: Duration, miri_flags: Vec<String>) -> Self {
        Self {
            image_name,
            timeout_duration,
            miri_flags,
        }
    }

    /// Check if Docker daemon is available and accessible
    pub async fn check_docker_available() -> Result<bool, VerifyError> {
        let output = Command::new("docker")
            .arg("version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await;

        match output {
            Ok(o) => Ok(o.status.success()),
            Err(_) => Ok(false),
        }
    }
}

#[async_trait]
impl ProofRunner for DockerMiriRunner {
    async fn run_miri(&self, package_dir: &Path, filter: &str) -> Result<MiriOutput, VerifyError> {
        if !Self::check_docker_available().await? {
            return Err(VerifyError::Sandbox(
                "Docker daemon is not accessible. Ensure Docker is running or fall back to LocalMiriRunner.".to_string(),
            ));
        }

        let abs_path = package_dir.canonicalize()?;
        let mount_arg = format!("{}:/workspace:ro", abs_path.display());
        let start_time = Instant::now();

        let mut cmd = Command::new("docker");
        cmd.arg("run")
            .arg("--rm")
            .arg("--network")
            .arg("none")
            .arg("--read-only")
            .arg("--tmpfs")
            .arg("/tmp:rw,noexec,nosuid")
            .arg("--pids-limit")
            .arg("64")
            .arg("--memory")
            .arg("2g")
            .arg("-v")
            .arg(&mount_arg)
            .arg("-w")
            .arg("/workspace");

        if !self.miri_flags.is_empty() {
            cmd.arg("-e")
                .arg(format!("MIRIFLAGS={}", self.miri_flags.join(" ")));
        }

        cmd.arg(&self.image_name)
            .arg("cargo")
            .arg("miri")
            .arg("test")
            .arg("--lib")
            .arg(filter)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let child = cmd.spawn()?;
        match timeout(self.timeout_duration, child.wait_with_output()).await {
            Ok(Ok(output)) => {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                Ok(MiriOutput {
                    exit_code: output.status.code(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    duration_ms,
                })
            }
            Ok(Err(e)) => Err(VerifyError::Io(e)),
            Err(_) => Err(VerifyError::Timeout(self.timeout_duration)),
        }
    }
}

/// Mock runner for unit tests and offline CI simulations
#[derive(Debug, Clone)]
pub struct MockProofRunner {
    pub canned_output: MiriOutput,
    pub should_timeout: bool,
}

impl MockProofRunner {
    pub fn new(canned_output: MiriOutput) -> Self {
        Self {
            canned_output,
            should_timeout: false,
        }
    }

    pub fn with_timeout() -> Self {
        Self {
            canned_output: MiriOutput {
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 0,
            },
            should_timeout: true,
        }
    }
}

#[async_trait]
impl ProofRunner for MockProofRunner {
    async fn run_miri(&self, _package_dir: &Path, _filter: &str) -> Result<MiriOutput, VerifyError> {
        if self.should_timeout {
            Err(VerifyError::Timeout(Duration::from_millis(50)))
        } else {
            Ok(self.canned_output.clone())
        }
    }

    async fn check_compilation(&self, _package_dir: &Path) -> Result<MiriOutput, VerifyError> {
        if self.should_timeout {
            Err(VerifyError::Timeout(Duration::from_millis(50)))
        } else {
            Ok(MiriOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 10,
            })
        }
    }

    async fn run_native_test(&self, _package_dir: &Path) -> Result<MiriOutput, VerifyError> {
        if self.should_timeout {
            Err(VerifyError::Timeout(Duration::from_millis(50)))
        } else {
            Ok(MiriOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 10,
            })
        }
    }
}
