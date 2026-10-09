//! Install and remove the official ChatGPT desktop package on Ubuntu / Debian.
//!
//! The Linux desktop app is published as `chatgpt_amd64.deb` / `chatgpt_arm64.deb`.
//! After it is installed, the existing `~/.codex` gateway config applies.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use serde::Serialize;

const DEB_HOST: &str = "persistent.oaistatic.com";
const AMD64_URL: &str =
    "https://persistent.oaistatic.com/codex-app-prod/linux/deb/latest/chatgpt_amd64.deb";
const ARM64_URL: &str =
    "https://persistent.oaistatic.com/codex-app-prod/linux/deb/latest/chatgpt_arm64.deb";
const MIN_DEB_BYTES: u64 = 5_000_000;
const PACKAGE_NAME: &str = "chatgpt";

static BUSY: AtomicBool = AtomicBool::new(false);

pub struct BusyGuard;

impl BusyGuard {
    pub fn try_begin() -> Result<Self, String> {
        if BUSY
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Err("已经在安装或卸载，请等这一次结束。".into());
        }
        Ok(Self)
    }
}

impl Drop for BusyGuard {
    fn drop(&mut self) {
        BUSY.store(false, Ordering::SeqCst);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DebAsset {
    pub filename: &'static str,
    pub url: &'static str,
    pub arch_label: &'static str,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinuxCodexStatus {
    pub installed: bool,
    pub version: Option<String>,
    pub arch: String,
}

pub fn official_deb(rust_arch: &str) -> Result<DebAsset, String> {
    match rust_arch {
        "x86_64" => Ok(DebAsset {
            filename: "chatgpt_amd64.deb",
            url: AMD64_URL,
            arch_label: "amd64",
        }),
        "aarch64" => Ok(DebAsset {
            filename: "chatgpt_arm64.deb",
            url: ARM64_URL,
            arch_label: "arm64",
        }),
        other => Err(format!(
            "这台电脑的架构是 {other}，官方 Ubuntu 安装包只提供 x64 和 ARM64"
        )),
    }
}

pub fn is_deb_header(bytes: &[u8]) -> bool {
    bytes.starts_with(b"!<arch>\n")
}

pub fn read_status() -> Result<LinuxCodexStatus, String> {
    let deb = official_deb(std::env::consts::ARCH)?;
    let version = installed_version();
    Ok(LinuxCodexStatus {
        installed: version.is_some(),
        version,
        arch: deb.arch_label.to_string(),
    })
}

pub fn installed_version() -> Option<String> {
    let output = Command::new("dpkg-query")
        .args(["-W", "-f", "${Version}", PACKAGE_NAME])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if version.is_empty() {
        None
    } else {
        Some(version)
    }
}

pub fn chatgpt_running() -> bool {
    ["chatgpt", "ChatGPT"].iter().any(|name| {
        Command::new("pgrep")
            .args(["-x", name])
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    })
}

pub fn download_path(filename: &str) -> PathBuf {
    std::env::temp_dir()
        .join("codex-app-manager")
        .join(filename)
}

pub async fn download_deb(asset: &DebAsset, dest: &Path) -> Result<(), String> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|error| format!("无法创建临时目录：{error}"))?;
    }
    let client = crate::app::http_client::builder()
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(30 * 60))
        .build()
        .map_err(|_| "无法开始下载".to_string())?;
    let mut response = client
        .get(asset.url)
        .send()
        .await
        .map_err(|_| "下载官方安装包失败，请检查网络".to_string())?;
    if !response.status().is_success() {
        return Err(format!("下载官方安装包失败（{}）", response.status()));
    }
    if response.url().host_str() != Some(DEB_HOST) {
        return Err("下载地址跳到了别的网站，已停止。".into());
    }
    let mut file =
        std::fs::File::create(dest).map_err(|error| format!("无法保存安装包：{error}"))?;
    let mut total = 0u64;
    while let Some(chunk) = response.chunk().await.map_err(|_| "下载中断".to_string())? {
        file.write_all(&chunk)
            .map_err(|error| format!("保存安装包失败：{error}"))?;
        total += chunk.len() as u64;
    }
    file.flush()
        .map_err(|error| format!("保存安装包失败：{error}"))?;
    if total < MIN_DEB_BYTES {
        let _ = std::fs::remove_file(dest);
        return Err("下载到的文件太小，不是完整的安装包。".into());
    }
    let mut header = [0u8; 8];
    let mut saved =
        std::fs::File::open(dest).map_err(|error| format!("无法检查安装包：{error}"))?;
    saved
        .read_exact(&mut header)
        .map_err(|_| "无法检查安装包".to_string())?;
    if !is_deb_header(&header) {
        let _ = std::fs::remove_file(dest);
        return Err("下载到的不是 Ubuntu 安装包，已停止。".into());
    }
    Ok(())
}

pub fn install_downloaded_deb(path: &Path) -> Result<(), String> {
    let text = path
        .to_str()
        .filter(|value| !value.is_empty())
        .ok_or("安装包路径无效")?;
    if !text.ends_with(".deb") || text.contains("..") {
        return Err("安装包路径无效".into());
    }
    run_as_root(&["/usr/bin/apt-get", "install", "-y", text])
}

pub fn uninstall_package(keep_data: bool) -> Result<(), String> {
    if chatgpt_running() {
        return Err("Codex 正在运行。请先完全退出，再卸载。".into());
    }
    run_as_root(&["/usr/bin/apt-get", "remove", "-y", PACKAGE_NAME])?;
    if !keep_data {
        remove_codex_home()?;
    }
    Ok(())
}

pub fn launch() -> Result<(), String> {
    let binary = if Path::new("/usr/bin/chatgpt").exists() {
        "/usr/bin/chatgpt"
    } else {
        "chatgpt"
    };
    Command::new(binary)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map_err(|error| format!("打不开 Codex：{error}。请确认已经安装。"))?;
    Ok(())
}

fn remove_codex_home() -> Result<(), String> {
    let Some(home) = crate::app::paths::codex_home_dir() else {
        return Ok(());
    };
    if home.file_name().and_then(|name| name.to_str()) != Some(".codex") {
        return Err("配置目录不是 ~/.codex，已停止，没有删除。".into());
    }
    if home.exists() {
        std::fs::remove_dir_all(&home)
            .map_err(|error| format!("应用已卸载，但没有删掉配置：{error}"))?;
    }
    Ok(())
}

fn run_as_root(args: &[&str]) -> Result<(), String> {
    if !Path::new("/usr/bin/apt-get").exists() {
        return Err("目前只支持 Ubuntu 和 Debian。这台电脑没有 apt。".into());
    }
    if !Path::new("/usr/bin/pkexec").exists() {
        return Err("这台电脑没有图形密码窗口，没法继续。请安装 pkexec 后再试。".into());
    }
    let output = Command::new("/usr/bin/pkexec")
        .args(args)
        .output()
        .map_err(|error| format!("无法请求管理员权限：{error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let detail = String::from_utf8_lossy(&output.stderr);
    let detail = detail.trim();
    let code = output.status.code().unwrap_or(-1);
    if detail.is_empty() || code == 126 || code == 127 {
        return Err("已取消，或没有拿到这台电脑的密码。".into());
    }
    let clipped: String = detail.chars().take(400).collect();
    Err(format!("没有完成：{clipped}"))
}

#[cfg(test)]
mod tests {
    use super::{is_deb_header, official_deb};

    #[test]
    fn maps_the_two_ubuntu_architectures() {
        let amd64 = official_deb("x86_64").unwrap();
        assert_eq!(amd64.filename, "chatgpt_amd64.deb");
        assert_eq!(amd64.arch_label, "amd64");
        assert!(amd64.url.ends_with("/chatgpt_amd64.deb"));

        let arm64 = official_deb("aarch64").unwrap();
        assert_eq!(arm64.filename, "chatgpt_arm64.deb");
        assert_eq!(arm64.arch_label, "arm64");
        assert!(arm64.url.ends_with("/chatgpt_arm64.deb"));
    }

    #[test]
    fn rejects_other_architectures() {
        let error = official_deb("i686").unwrap_err();
        assert!(error.contains("i686"));
    }

    #[test]
    fn recognizes_a_debian_package_header() {
        assert!(is_deb_header(b"!<arch>\n"));
        assert!(!is_deb_header(b"!<arch>"));
        assert!(!is_deb_header(b"PK\x03\x04"));
    }
}
