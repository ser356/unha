use anyhow::{anyhow, Context, Result};
use once_cell::sync::OnceCell;
use regex::Regex;
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager, Window};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::Mutex as AsyncMutex;
use yt_dlp::client::deps::LibraryInstaller;

use crate::updater;

#[derive(Debug, Clone, Copy)]
pub enum AudioFormat {
    Wav,
    Aiff,
    Mp3,
}

impl AudioFormat {
    pub fn parse(s: &str) -> Result<Self> {
        match s.to_ascii_lowercase().as_str() {
            "wav" => Ok(Self::Wav),
            "aiff" => Ok(Self::Aiff),
            "mp3" => Ok(Self::Mp3),
            other => Err(anyhow!("formato no soportado: {other}")),
        }
    }
    fn as_str(&self) -> &'static str {
        match self {
            Self::Wav => "wav",
            Self::Aiff => "aiff",
            Self::Mp3 => "mp3",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct Entry {
    pub index: u32,
    pub id: String,
    pub title: String,
    pub url: Option<String>,
    pub thumbnail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct Probe {
    pub kind: &'static str,
    pub title: String,
    pub entries: Vec<Entry>,
}

#[derive(Debug, Serialize, Clone)]
struct ProgressPayload {
    index: u32,
    percent: f32,
    status: String,
    title: String,
}

struct Libs {
    yt_dlp: PathBuf,
    ffmpeg: PathBuf,
}

static LIBS: OnceCell<Mutex<Option<Libs>>> = OnceCell::new();
static INSTALL_LOCK: OnceCell<AsyncMutex<()>> = OnceCell::new();

fn cell() -> &'static Mutex<Option<Libs>> {
    LIBS.get_or_init(|| Mutex::new(None))
}

fn install_lock() -> &'static AsyncMutex<()> {
    INSTALL_LOCK.get_or_init(|| AsyncMutex::new(()))
}

fn libs_dir(app: &AppHandle) -> Result<PathBuf> {
    let base = app.path().app_data_dir().context("app_data_dir")?;
    let d = base.join("libs");
    std::fs::create_dir_all(&d).with_context(|| format!("crear {}", d.display()))?;
    Ok(d)
}

fn ext_bin(name: &str) -> String {
    #[cfg(windows)]
    {
        format!("{name}.exe")
    }
    #[cfg(not(windows))]
    {
        name.to_string()
    }
}

async fn probe_binary(cmd: &Path) -> bool {
    updater::hidden_command(cmd)
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
        .map(|s| s.success())
        .unwrap_or(false)
}

async fn find_in_path(name: &str) -> Option<PathBuf> {
    let bin = ext_bin(name);
    let candidate = PathBuf::from(&bin);
    if probe_binary(&candidate).await {
        return Some(candidate);
    }
    None
}

async fn resolve_or_install(app: &AppHandle) -> Result<(PathBuf, PathBuf)> {
    let _guard = install_lock().lock().await;
    if let Some(l) = cell().lock().unwrap().as_ref() {
        return Ok((l.yt_dlp.clone(), l.ffmpeg.clone()));
    }

    let dir = libs_dir(app)?;

    // yt-dlp: siempre bundleado, nightly, refresh cada 24h.
    let ytdlp = updater::ensure_ytdlp(&dir, Duration::from_secs(24 * 3600))
        .await
        .context("ensure yt-dlp nightly")?;

    // ffmpeg: system-first, fallback al installer del crate (más estable, rota poco).
    let bundled_ffmpeg = dir.join(ext_bin("ffmpeg"));
    let ffmpeg = match find_in_path("ffmpeg").await {
        Some(p) => p,
        None => {
            if !bundled_ffmpeg.exists() {
                let installer = LibraryInstaller::new(dir.clone());
                installer
                    .install_ffmpeg(None)
                    .await
                    .map_err(|e| anyhow!("instalar ffmpeg: {e}"))?;
            }
            if !bundled_ffmpeg.exists() {
                return Err(anyhow!("no encuentro ffmpeg tras la instalación"));
            }
            bundled_ffmpeg
        }
    };

    *cell().lock().unwrap() = Some(Libs {
        yt_dlp: ytdlp.clone(),
        ffmpeg: ffmpeg.clone(),
    });
    Ok((ytdlp, ffmpeg))
}

pub async fn force_update_ytdlp(app: &AppHandle) -> Result<String> {
    let _guard = install_lock().lock().await;
    let dir = libs_dir(app)?;
    let ytdlp = updater::force_update(&dir)
        .await
        .context("force update yt-dlp")?;
    // invalida cache para recoger nuevo path
    *cell().lock().unwrap() = None;
    updater::current_version(&ytdlp).await
}

pub async fn ytdlp_version(app: &AppHandle) -> Result<String> {
    let (ytdlp, _) = resolve_or_install(app).await?;
    updater::current_version(&ytdlp).await
}

pub async fn ensure_libs(app: &AppHandle) -> Result<()> {
    resolve_or_install(app).await?;
    Ok(())
}

pub async fn probe(
    app: &AppHandle,
    url: &str,
    cookies_from_browser: Option<&str>,
) -> Result<Probe> {
    let url = validate_url(url)?;
    let url = url.as_str();
    let (ytdlp, _ffmpeg) = resolve_or_install(app).await?;

    let mut args: Vec<String> = vec![
        "--dump-single-json".into(),
        "--flat-playlist".into(),
        "--no-warnings".into(),
        "--ignore-config".into(),
    ];
    if let Some(browser) = cookies_from_browser {
        validate_browser(browser)?;
        args.push("--cookies-from-browser".into());
        args.push(browser.to_string());
    }
    args.push("--".into());
    args.push(url.to_string());

    let output = updater::hidden_command(&ytdlp)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
        .context("spawn yt-dlp probe")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!("yt-dlp probe falló: {}", stderr.trim()));
    }

    let v: serde_json::Value = serde_json::from_slice(&output.stdout).context("parsear JSON")?;
    let title = v
        .get("title")
        .and_then(|x| x.as_str())
        .unwrap_or("(sin título)")
        .to_string();

    if let Some(arr) = v.get("entries").and_then(|x| x.as_array()) {
        let entries = arr
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let id = e.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
                let title = e
                    .get("title")
                    .and_then(|x| x.as_str())
                    .unwrap_or(&id)
                    .to_string();
                let url = e.get("url").and_then(|x| x.as_str()).map(String::from);
                let thumbnail = pick_thumbnail(e);
                if id.is_empty() && url.is_none() {
                    None
                } else {
                    Some(Entry {
                        index: (i as u32) + 1,
                        id,
                        title,
                        url,
                        thumbnail,
                    })
                }
            })
            .collect();
        Ok(Probe {
            kind: "playlist",
            title,
            entries,
        })
    } else {
        let id = v.get("id").and_then(|x| x.as_str()).unwrap_or("").to_string();
        let thumbnail = pick_thumbnail(&v);
        Ok(Probe {
            kind: "single",
            title: title.clone(),
            entries: vec![Entry {
                index: 1,
                id,
                title,
                url: Some(url.to_string()),
                thumbnail,
            }],
        })
    }
}

fn pick_thumbnail(v: &serde_json::Value) -> Option<String> {
    if let Some(s) = v.get("thumbnail").and_then(|x| x.as_str()) {
        if !s.is_empty() {
            return Some(s.to_string());
        }
    }
    v.get("thumbnails")
        .and_then(|x| x.as_array())
        .and_then(|arr| {
            arr.iter()
                .filter_map(|t| {
                    let url = t.get("url").and_then(|x| x.as_str())?;
                    let w = t.get("width").and_then(|x| x.as_i64()).unwrap_or(0);
                    Some((w, url.to_string()))
                })
                .max_by_key(|(w, _)| *w)
                .map(|(_, u)| u)
        })
}

pub async fn download(
    app: &AppHandle,
    window: &Window,
    url: &str,
    format: AudioFormat,
    mp3_bitrate: Option<u16>,
    dest_dir: &str,
    indices: &[u32],
    cookies_from_browser: Option<&str>,
) -> Result<()> {
    let url = validate_url(url)?;
    let url = url.as_str();
    let (ytdlp, ffmpeg) = resolve_or_install(app).await?;

    let dest = Path::new(dest_dir);
    if !dest.is_dir() {
        return Err(anyhow!(
            "carpeta destino inválida: {}",
            dest.display()
        ));
    }
    let ffmpeg_dir = ffmpeg
        .parent()
        .ok_or_else(|| anyhow!("directorio de ffmpeg"))?;

    let tmp_root = std::env::temp_dir().join(format!("unha-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_root).with_context(|| format!("crear {}", tmp_root.display()))?;

    let mut args: Vec<String> = vec![
        "-x".into(),
        "--audio-format".into(),
        format.as_str().into(),
        "--no-playlist-reverse".into(),
        "--no-warnings".into(),
        "--ignore-config".into(),
        "--newline".into(),
        "--progress".into(),
        "--progress-template".into(),
        "download:PROG|%(info.playlist_index|1)s|%(progress._percent_str)s|%(info.title)s".into(),
        "--ffmpeg-location".into(),
        ffmpeg_dir.to_string_lossy().into_owned(),
        "-P".into(),
        format!("home:{}", dest.to_string_lossy()),
        "-P".into(),
        format!("temp:{}", tmp_root.to_string_lossy()),
        "-o".into(),
        "%(title)s.%(ext)s".into(),
    ];

    if let Some(browser) = cookies_from_browser {
        validate_browser(browser)?;
        args.push("--cookies-from-browser".into());
        args.push(browser.to_string());
    }

    if matches!(format, AudioFormat::Mp3) {
        let br = mp3_bitrate.unwrap_or(192);
        if !matches!(br, 128 | 192 | 256 | 320) {
            return Err(anyhow!("bitrate MP3 inválido: {br}"));
        }
        args.push("--audio-quality".into());
        args.push(format!("{br}K"));
    }

    if !indices.is_empty() {
        let sel = indices
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(",");
        args.push("--playlist-items".into());
        args.push(sel);
    }

    args.push("--".into());
    args.push(url.to_string());

    let mut child = updater::hidden_command(&ytdlp)
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn yt-dlp download")?;

    let stdout = child.stdout.take().ok_or_else(|| anyhow!("stdout"))?;
    let stderr = child.stderr.take().ok_or_else(|| anyhow!("stderr"))?;

    let w1 = window.clone();
    let w2 = window.clone();
    let re = Regex::new(r"^PROG\|(\d+)\|\s*([\d.]+)%\|(.*)$").unwrap();

    let t_out = tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if let Some(caps) = re.captures(&line) {
                let idx: u32 = caps[1].parse().unwrap_or(1);
                let pct: f32 = caps[2].parse().unwrap_or(0.0);
                let title = caps[3].trim().to_string();
                let _ = w1.emit(
                    "dl://progress",
                    ProgressPayload {
                        index: idx,
                        percent: pct.clamp(0.0, 100.0),
                        status: format!("{pct:.1}%"),
                        title,
                    },
                );
            } else if !line.is_empty() {
                let _ = w1.emit("dl://log", line);
            }
        }
    });

    let t_err = tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            if !line.is_empty() {
                let _ = w2.emit("dl://log", line);
            }
        }
    });

    let status = child.wait().await.context("esperar yt-dlp")?;
    let _ = t_out.await;
    let _ = t_err.await;

    if !status.success() {
        return Err(anyhow!(
            "yt-dlp terminó con código {}",
            status.code().unwrap_or(-1)
        ));
    }
    Ok(())
}

fn validate_url(input: &str) -> Result<String> {
    let mut parsed = url::Url::parse(input).map_err(|e| anyhow!("URL inválida: {e}"))?;
    match parsed.scheme() {
        "http" | "https" => {}
        other => return Err(anyhow!("esquema no permitido: {other}")),
    }

    // YouTube auto-append `&list=RD…` (Mix / radio) cuando pinchas en un vídeo
    // que forma parte de un mix. Esos playlists son dinámicos/infinitos y
    // yt-dlp los intenta enumerar hasta timeout → probe cuelga eterno.
    // Si la URL trae un `v=` explícito, quitamos SOLO los `list=RD*` y
    // dejamos intactos los playlists reales (PL*, LL, WL, RDCLAK…).
    let host_is_youtube = parsed
        .host_str()
        .map(|h| h.ends_with("youtube.com") || h == "youtu.be")
        .unwrap_or(false);
    if host_is_youtube {
        let has_v = parsed.query_pairs().any(|(k, _)| k == "v");
        let has_radio_list = parsed
            .query_pairs()
            .any(|(k, v)| k == "list" && v.starts_with("RD"));
        if has_v && has_radio_list {
            let kept: Vec<(String, String)> = parsed
                .query_pairs()
                .filter(|(k, v)| !(k == "list" && v.starts_with("RD"))
                    && k != "start_radio"
                    && k != "index")
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            parsed.query_pairs_mut().clear().extend_pairs(kept.iter());
            if parsed.query().map(|q| q.is_empty()).unwrap_or(false) {
                parsed.set_query(None);
            }
        }
    }

    Ok(parsed.to_string())
}

fn validate_browser(name: &str) -> Result<()> {
    match name {
        "chrome" | "chromium" | "firefox" | "safari" | "edge" | "brave" | "vivaldi"
        | "opera" => Ok(()),
        other => Err(anyhow!("navegador no soportado: {other}")),
    }
}
