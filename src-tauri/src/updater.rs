use anyhow::{anyhow, Context, Result};
use futures_util::StreamExt;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime};
use tokio::io::AsyncWriteExt;
use tokio::process::Command;

const NIGHTLY_BASE: &str =
    "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download";

fn nightly_asset() -> Result<&'static str> {
    #[cfg(all(target_os = "macos"))]
    {
        Ok("yt-dlp_macos")
    }
    #[cfg(all(target_os = "windows"))]
    {
        Ok("yt-dlp.exe")
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        Ok("yt-dlp_linux")
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        Ok("yt-dlp_linux_aarch64")
    }
    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        all(target_os = "linux", any(target_arch = "x86_64", target_arch = "aarch64"))
    )))]
    {
        Err(anyhow!("plataforma no soportada para yt-dlp nightly"))
    }
}

pub fn ytdlp_path(dir: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        dir.join("yt-dlp.exe")
    }
    #[cfg(not(windows))]
    {
        dir.join("yt-dlp")
    }
}

fn is_fresh(path: &Path, max_age: Duration) -> bool {
    let Ok(md) = std::fs::metadata(path) else {
        return false;
    };
    if md.len() < 1024 {
        return false;
    }
    let Ok(modified) = md.modified() else {
        return false;
    };
    SystemTime::now()
        .duration_since(modified)
        .map(|age| age < max_age)
        .unwrap_or(false)
}

pub async fn ensure_ytdlp(dir: &Path, max_age: Duration) -> Result<PathBuf> {
    let target = ytdlp_path(dir);
    if is_fresh(&target, max_age) {
        return Ok(target);
    }
    fetch_nightly(&target).await?;
    Ok(target)
}

pub async fn force_update(dir: &Path) -> Result<PathBuf> {
    let target = ytdlp_path(dir);
    fetch_nightly(&target).await?;
    Ok(target)
}

async fn fetch_nightly(target: &Path) -> Result<()> {
    let asset = nightly_asset()?;
    let url = format!("{NIGHTLY_BASE}/{asset}");

    let parent = target
        .parent()
        .ok_or_else(|| anyhow!("target sin directorio padre"))?;
    tokio::fs::create_dir_all(parent).await?;
    let part = target.with_extension("part");

    let client = reqwest::Client::builder()
        .user_agent("unha-updater/0.1")
        .connect_timeout(Duration::from_secs(15))
        .build()?;

    let resp = client
        .get(&url)
        .send()
        .await
        .with_context(|| format!("GET {url}"))?
        .error_for_status()
        .with_context(|| format!("HTTP status en {url}"))?;

    let mut stream = resp.bytes_stream();
    let mut file = tokio::fs::File::create(&part)
        .await
        .with_context(|| format!("crear {}", part.display()))?;
    while let Some(chunk) = stream.next().await {
        let bytes = chunk.context("stream chunk")?;
        file.write_all(&bytes).await.context("escribir chunk")?;
    }
    file.flush().await?;
    drop(file);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = tokio::fs::metadata(&part).await?.permissions();
        perms.set_mode(0o755);
        tokio::fs::set_permissions(&part, perms).await?;
    }

    tokio::fs::rename(&part, target)
        .await
        .with_context(|| format!("rename {} -> {}", part.display(), target.display()))?;

    Ok(())
}

pub async fn current_version(ytdlp: &Path) -> Result<String> {
    let out = Command::new(ytdlp)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .await
        .with_context(|| format!("spawn {} --version", ytdlp.display()))?;
    if !out.status.success() {
        return Err(anyhow!(
            "yt-dlp --version salió con código {}",
            out.status.code().unwrap_or(-1)
        ));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}
