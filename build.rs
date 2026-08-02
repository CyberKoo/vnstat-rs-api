use std::process::Command;

/// Embeds the git branch and short commit hash into the binary so builds
/// can be identified at runtime (e.g. `vnstat-rs-api 1.2.0-dev@9bfa843`).
///
/// `GIT_DEV_SUFFIX` is the banner suffix for non-release builds (`-dev`),
/// or empty for release builds. When the build happens outside a git
/// checkout (release tarballs, CI without git), all variables are empty and
/// the version banner falls back to the plain package version.
fn main() {
    let branch = git_output(&["branch", "--show-current"]);
    let commit = git_output(&["rev-parse", "--short", "HEAD"]);
    let is_dev = !branch.is_empty() && branch != "master";

    println!("cargo:rustc-env=GIT_BRANCH={branch}");
    println!("cargo:rustc-env=GIT_COMMIT={commit}");
    println!(
        "cargo:rustc-env=GIT_DEV_SUFFIX={}",
        if is_dev { "-dev" } else { "" }
    );

    // Re-run the build script when this file changes, when the checked-out
    // branch changes, or when the branch ref moves (new commits).
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=.git/HEAD");
    if !branch.is_empty() {
        println!("cargo:rerun-if-changed=.git/refs/heads/{branch}");
    }
}

/// Runs `git <args>` and returns the trimmed stdout, or an empty string
/// when git is unavailable or the command fails (not a git checkout).
fn git_output(args: &[&str]) -> String {
    Command::new("git")
        .args(args)
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
        .unwrap_or_default()
}
