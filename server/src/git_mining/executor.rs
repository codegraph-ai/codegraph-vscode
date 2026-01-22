//! Git command execution wrapper.

use super::GitMiningError;
use std::path::Path;
use std::process::Command;

/// Wrapper for executing git commands.
pub struct GitExecutor {
    repo_path: std::path::PathBuf,
}

impl GitExecutor {
    /// Create a new git executor for the given repository path.
    pub fn new(repo_path: &Path) -> Result<Self, GitMiningError> {
        // Verify git is available
        let output = Command::new("git").arg("--version").output()?;

        if !output.status.success() {
            return Err(GitMiningError::GitNotAvailable);
        }

        // Verify path is a git repository
        let output = Command::new("git")
            .current_dir(repo_path)
            .args(["rev-parse", "--git-dir"])
            .output()?;

        if !output.status.success() {
            return Err(GitMiningError::NotARepository(repo_path.to_path_buf()));
        }

        Ok(Self {
            repo_path: repo_path.to_path_buf(),
        })
    }

    /// Get commit log with custom format.
    ///
    /// Format placeholders:
    /// - %H: commit hash
    /// - %s: subject
    /// - %b: body
    /// - %an: author name
    /// - %ae: author email
    /// - %ai: author date (ISO format)
    pub fn log(
        &self,
        format: &str,
        limit: Option<usize>,
        path_filter: Option<&Path>,
    ) -> Result<String, GitMiningError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.repo_path);
        cmd.args(["log", &format!("--format={}", format)]);

        if let Some(n) = limit {
            cmd.arg(format!("-n{}", n));
        }

        cmd.arg("--");

        if let Some(path) = path_filter {
            cmd.arg(path);
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitMiningError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8(output.stdout)?)
    }

    /// Get commits matching a grep pattern in commit messages.
    pub fn log_grep(
        &self,
        pattern: &str,
        format: &str,
        limit: Option<usize>,
    ) -> Result<String, GitMiningError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.repo_path);
        cmd.args([
            "log",
            &format!("--format={}", format),
            "--all",
            "-i", // case insensitive
            &format!("--grep={}", pattern),
        ]);

        if let Some(n) = limit {
            cmd.arg(format!("-n{}", n));
        }

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitMiningError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8(output.stdout)?)
    }

    /// Get the files changed in a specific commit.
    pub fn show_files(&self, commit_hash: &str) -> Result<Vec<String>, GitMiningError> {
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args(["show", "--name-only", "--format=", commit_hash])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitMiningError::CommandFailed(stderr.to_string()));
        }

        let stdout = String::from_utf8(output.stdout)?;
        Ok(stdout
            .lines()
            .filter(|l| !l.is_empty())
            .map(String::from)
            .collect())
    }

    /// Get the diff statistics for a commit.
    pub fn show_stat(&self, commit_hash: &str) -> Result<String, GitMiningError> {
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args(["show", "--stat", "--format=", commit_hash])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitMiningError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8(output.stdout)?)
    }

    /// Get the full commit message for a specific commit.
    pub fn show_message(&self, commit_hash: &str) -> Result<String, GitMiningError> {
        let output = Command::new("git")
            .current_dir(&self.repo_path)
            .args(["show", "-s", "--format=%B", commit_hash])
            .output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitMiningError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8(output.stdout)?.trim().to_string())
    }

    /// Get git blame for a specific file.
    pub fn blame(
        &self,
        path: &Path,
        line_range: Option<(u32, u32)>,
    ) -> Result<String, GitMiningError> {
        let mut cmd = Command::new("git");
        cmd.current_dir(&self.repo_path);
        cmd.args(["blame", "--porcelain"]);

        if let Some((start, end)) = line_range {
            cmd.arg(format!("-L{},{}", start, end));
        }

        cmd.arg(path);

        let output = cmd.output()?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(GitMiningError::CommandFailed(stderr.to_string()));
        }

        Ok(String::from_utf8(output.stdout)?)
    }

    /// Get repository root path.
    pub fn repo_path(&self) -> &Path {
        &self.repo_path
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn test_git_executor_creation() {
        // This test only works if run from within a git repository
        let current_dir = env::current_dir().unwrap();
        let result = GitExecutor::new(&current_dir);
        // May or may not succeed depending on where tests are run
        if let Ok(executor) = result {
            assert!(executor.repo_path().exists());
        }
    }
}
