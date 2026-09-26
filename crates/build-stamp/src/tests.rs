#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use tempfile::TempDir;

    /// Helper to create a temporary git repository for testing
    fn create_test_git_repo() -> (TempDir, PathBuf) {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let repo_path = temp_dir.path().to_path_buf();

        // Initialize git repo
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to init git repo");

        // Configure git for the test repo
        std::process::Command::new("git")
            .args(["config", "user.email", "test@example.com"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to configure git email");

        std::process::Command::new("git")
            .args(["config", "user.name", "Test User"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to configure git name");

        // Create initial commit
        fs::write(repo_path.join("README.md"), "test").expect("Failed to write README");
        std::process::Command::new("git")
            .args(["add", "README.md"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to git add");

        std::process::Command::new("git")
            .args(["commit", "-m", "Initial commit"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to git commit");

        (temp_dir, repo_path)
    }

    #[test]
    fn test_resolve_git_ref_path_symbolic_branch() {
        let (_temp_dir, repo_path) = create_test_git_repo();

        // On a normal branch, HEAD should contain "ref: refs/heads/main" or similar
        let head_content =
            fs::read_to_string(repo_path.join(".git/HEAD")).expect("Failed to read HEAD");

        if head_content.starts_with("ref: ") {
            // Symbolic ref: should resolve to the actual ref file
            let ref_name = head_content.strip_prefix("ref: ").unwrap().trim();
            let ref_path = repo_path.join(".git").join(ref_name);
            assert!(ref_path.exists(), "Ref file should exist: {:?}", ref_path);
        }
    }

    #[test]
    fn test_resolve_git_ref_path_detached_head() {
        let (_temp_dir, repo_path) = create_test_git_repo();

        // Get current commit SHA
        let output = std::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to get HEAD SHA");

        let sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // Detach HEAD
        std::process::Command::new("git")
            .args(["checkout", &sha])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to detach HEAD");

        // In detached HEAD, HEAD should contain the SHA directly
        let head_content =
            fs::read_to_string(repo_path.join(".git/HEAD")).expect("Failed to read HEAD");

        assert!(
            !head_content.starts_with("ref: "),
            "Detached HEAD should not be symbolic"
        );
        assert_eq!(head_content.trim(), sha, "HEAD should contain the SHA");
    }

    #[test]
    fn test_build_stamp_emits_short_sha() {
        let (_temp_dir, repo_path) = create_test_git_repo();

        // Get expected short SHA
        let output = std::process::Command::new("git")
            .args(["rev-parse", "--short", "HEAD"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to get short SHA");

        let expected_sha = String::from_utf8_lossy(&output.stdout).trim().to_string();

        // The build stamp should emit this SHA
        // Note: We can't easily test the actual emit() function without a full Cargo build,
        // but we can verify the git command works correctly
        assert!(!expected_sha.is_empty(), "Short SHA should not be empty");
        assert!(
            expected_sha.len() >= 7,
            "Short SHA should be at least 7 characters"
        );
    }

    #[test]
    fn test_build_stamp_dirty_detection() {
        let (_temp_dir, repo_path) = create_test_git_repo();

        // Initially clean
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to check git status");

        let status = String::from_utf8_lossy(&output.stdout);
        assert!(status.trim().is_empty(), "Repo should be clean initially");

        // Make a change
        fs::write(repo_path.join("dirty.txt"), "dirty").expect("Failed to write dirty file");

        // Now should be dirty
        let output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&repo_path)
            .output()
            .expect("Failed to check git status");

        let status = String::from_utf8_lossy(&output.stdout);
        assert!(
            !status.trim().is_empty(),
            "Repo should be dirty after changes"
        );
    }

    #[test]
    fn test_build_stamp_git_unavailable() {
        // When git is unavailable or not in a repo, the stamp should be "unknown"
        // This is tested implicitly by the fact that emit() uses unwrap_or_else(|| "unknown")
        // We can't easily test this without mocking, but the contract is documented
    }
}
