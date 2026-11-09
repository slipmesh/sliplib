pub fn send_sigterm_to_this_process() -> anyhow::Result<()> {
    let status = std::process::Command::new("kill")
        .args(["-TERM", &std::process::id().to_string()])
        .status()?;
    anyhow::ensure!(status.success(), "kill exited with {status}");
    Ok(())
}
