//! Company gateway settings for the official Codex desktop app.
//!
//! Codex reads the user-level `config.toml` for the provider and model, and
//! reads the API key from `~/.codex/.env` via `env_key`. The key is never
//! written into `config.toml` and never returned to the UI.

use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use url::Url;

pub const DEFAULT_BASE_URL: &str = "https://aiapi.yxrobot.com/v1";
pub const PROVIDER_ID: &str = "yxrobot";
const ENV_KEY: &str = "YXROBOT_API_KEY";
const PROVIDER_NAME: &str = "公司 AI 网关";

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct CodexGatewayStatus {
    pub base_url: String,
    pub model: String,
    pub api_key_set: bool,
    pub codex_running: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexGatewayInput {
    pub base_url: String,
    pub model: String,
    /// Empty keeps the key already stored on this computer.
    pub api_key: String,
}

#[derive(Debug)]
pub struct GatewayError(String);

impl std::fmt::Display for GatewayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn read_gateway(codex_home: &Path) -> CodexGatewayStatus {
    let config = read_to_string_if_utf8(&config_path(codex_home)).unwrap_or_default();
    let env = read_to_string_if_utf8(&env_path(codex_home)).unwrap_or_default();
    let base_url = provider_base_url(&config).unwrap_or_else(|| DEFAULT_BASE_URL.to_string());
    CodexGatewayStatus {
        base_url,
        model: top_level_value(&config, "model").unwrap_or_default(),
        api_key_set: env_value(&env, ENV_KEY).is_some_and(|value| !value.is_empty()),
        codex_running: false,
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListModelsInput {
    pub base_url: String,
    /// Empty uses the key already stored on this computer.
    pub api_key: String,
}

pub fn resolve_gateway_key(codex_home: &Path, raw: &str) -> Result<String, GatewayError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return resolve_api_key(codex_home, "");
    }
    validate_api_key(trimmed)
}

pub fn parse_model_ids(body: &str) -> Result<Vec<String>, GatewayError> {
    #[derive(Deserialize)]
    struct Payload {
        data: Vec<ModelItem>,
    }
    #[derive(Deserialize)]
    struct ModelItem {
        id: String,
    }
    let payload: Payload = serde_json::from_str(body)
        .map_err(|_| GatewayError("网关没有返回可用模型列表".into()))?;
    let mut ids: Vec<String> = payload
        .data
        .into_iter()
        .map(|item| item.id.trim().to_string())
        .filter(|id| !id.is_empty())
        .collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Err(GatewayError("网关上没有可用模型".into()));
    }
    Ok(ids)
}

pub async fn fetch_model_ids(base_url: &str, api_key: &str) -> Result<Vec<String>, GatewayError> {
    let base_url = validate_base_url(base_url)?;
    let api_key = validate_api_key(api_key)?;
    let url = format!("{base_url}/models");
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .map_err(|_| GatewayError("无法连接网关".into()))?;
    let response = client
        .get(&url)
        .header("Authorization", format!("Bearer {api_key}"))
        .header("Accept", "application/json")
        .send()
        .await
        .map_err(|_| GatewayError("连接网关失败，请检查地址和网络".into()))?;
    let status = response.status();
    if !status.is_success() {
        return Err(GatewayError(format!("网关拒绝了模型列表（{status}）")));
    }
    let body = response
        .text()
        .await
        .map_err(|_| GatewayError("读取模型列表失败".into()))?;
    parse_model_ids(&body)
}

pub fn apply_gateway(codex_home: &Path, input: &CodexGatewayInput) -> Result<(), GatewayError> {
    let base_url = validate_base_url(&input.base_url)?;
    let model = validate_model(&input.model)?;
    let api_key = resolve_api_key(codex_home, &input.api_key)?;

    let config_text = read_to_string_if_utf8(&config_path(codex_home)).unwrap_or_default();
    let next_config = merge_config(&config_text, &model, &base_url);
    if next_config.contains(&api_key) {
        return Err(GatewayError("API Key 不能写入配置文件".into()));
    }
    replace_file(&config_path(codex_home), next_config.as_bytes(), 0o644)?;

    let env_text = read_to_string_if_utf8(&env_path(codex_home)).unwrap_or_default();
    let next_env = merge_env(&env_text, &api_key);
    replace_file(&env_path(codex_home), next_env.as_bytes(), 0o600)?;
    Ok(())
}

fn resolve_api_key(codex_home: &Path, raw: &str) -> Result<String, GatewayError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        let existing = read_to_string_if_utf8(&env_path(codex_home)).unwrap_or_default();
        return env_value(&existing, ENV_KEY)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| GatewayError("请填写 API Key".into()));
    }
    validate_api_key(trimmed)
}

fn validate_base_url(raw: &str) -> Result<String, GatewayError> {
    let raw = raw.trim();
    if raw.is_empty() {
        return Err(GatewayError("请填写网关地址".into()));
    }
    if raw.chars().any(|ch| ch.is_control() || ch.is_whitespace()) {
        return Err(GatewayError("网关地址不能包含空格".into()));
    }
    let mut url = Url::parse(raw).map_err(|_| GatewayError("网关地址无法识别，请以 https:// 开头".into()))?;
    if url.scheme() != "https" {
        return Err(GatewayError("网关地址必须是 https:// 链接".into()));
    }
    if !url.username().is_empty() || url.password().is_some() {
        return Err(GatewayError("网关地址不能包含账号或密码".into()));
    }
    if url.host_str().is_none() {
        return Err(GatewayError("网关地址缺少主机名".into()));
    }
    url.set_query(None);
    url.set_fragment(None);
    let mut text = url.to_string();
    if text.ends_with('/') {
        text.pop();
    }
    Ok(text)
}

fn validate_model(raw: &str) -> Result<String, GatewayError> {
    let model = raw.trim();
    if model.is_empty() {
        return Err(GatewayError("请填写默认模型".into()));
    }
    if model.chars().count() > 200
        || model
            .chars()
            .any(|ch| ch.is_control() || ch.is_whitespace() || matches!(ch, '"' | '\\' | '#' | '\''))
    {
        return Err(GatewayError("模型名只能包含普通字符，不能有空格或引号".into()));
    }
    Ok(model.to_string())
}

fn validate_api_key(raw: &str) -> Result<String, GatewayError> {
    if raw.chars().count() > 512
        || raw.chars().any(|ch| {
            ch.is_control() || ch.is_whitespace() || matches!(ch, '"' | '\'' | '#' | '\\')
        })
    {
        return Err(GatewayError("API Key 格式不正确".into()));
    }
    Ok(raw.to_string())
}

fn merge_config(current: &str, model: &str, base_url: &str) -> String {
    let (mut lines, newline) = split_lines(current);
    set_top_key(&mut lines, "model", &format!("model = \"{model}\""));
    set_top_key(
        &mut lines,
        "model_provider",
        &format!("model_provider = \"{PROVIDER_ID}\""),
    );
    replace_provider_section(&mut lines, base_url);
    join_lines(&lines, newline)
}

fn set_top_key(lines: &mut Vec<String>, key: &str, rendered: &str) {
    let top_end = top_scope_end(lines);
    let hits: Vec<usize> = (0..top_end)
        .filter(|&index| is_top_key(&lines[index], key))
        .collect();
    if let Some(first) = hits.first().copied() {
        lines[first] = rendered.to_string();
        for index in hits.into_iter().skip(1).rev() {
            lines.remove(index);
        }
        return;
    }
    let insert_at = top_scope_end(lines);
    lines.insert(insert_at, rendered.to_string());
}

fn replace_provider_section(lines: &mut Vec<String>, base_url: &str) {
    let block = provider_block(base_url);
    let mut index = 0;
    let mut replaced = false;
    while index < lines.len() {
        if !header_name(&lines[index]).is_some_and(is_our_section) {
            index += 1;
            continue;
        }
        let start = index;
        index += 1;
        while index < lines.len() && !header_name(&lines[index]).is_some_and(|name| !is_our_section(name))
        {
            index += 1;
        }
        if replaced {
            lines.drain(start..index);
            index = start;
            continue;
        }
        lines.splice(start..index, block.clone());
        replaced = true;
        index = start + block.len();
    }
    if replaced {
        return;
    }
    if lines.last().is_some_and(|line| !line.is_empty()) {
        lines.push(String::new());
    }
    lines.extend(block);
}

fn provider_block(base_url: &str) -> Vec<String> {
    vec![
        format!("[model_providers.{PROVIDER_ID}]"),
        format!("name = \"{PROVIDER_NAME}\""),
        format!("base_url = \"{base_url}\""),
        format!("env_key = \"{ENV_KEY}\""),
        "wire_api = \"responses\"".to_string(),
    ]
}

fn provider_base_url(config: &str) -> Option<String> {
    let (lines, _) = split_lines(config);
    let mut inside = false;
    for line in &lines {
        if let Some(name) = header_name(line) {
            inside = is_our_section(name);
            continue;
        }
        if inside && is_top_key(line, "base_url") {
            return assignment_value(line);
        }
    }
    None
}

fn top_level_value(config: &str, key: &str) -> Option<String> {
    let (lines, _) = split_lines(config);
    let top_end = top_scope_end(&lines);
    (0..top_end).find_map(|index| {
        is_top_key(&lines[index], key).then(|| assignment_value(&lines[index])).flatten()
    })
}

fn merge_env(current: &str, api_key: &str) -> String {
    let (mut lines, newline) = split_lines(current);
    let rendered = format!("{ENV_KEY}={api_key}");
    let hits: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, line)| env_line_key(line) == Some(ENV_KEY))
        .map(|(index, _)| index)
        .collect();
    if let Some(first) = hits.first().copied() {
        lines[first] = rendered;
        for index in hits.into_iter().skip(1).rev() {
            lines.remove(index);
        }
    } else if lines.iter().all(|line| line.is_empty()) {
        lines = vec![rendered];
    } else {
        if lines.last().is_some_and(|line| !line.is_empty()) {
            lines.push(String::new());
        }
        lines.push(rendered);
    }
    ensure_trailing_newline(join_lines(&lines, newline), newline)
}

fn env_value(text: &str, key: &str) -> Option<String> {
    let (lines, _) = split_lines(text);
    lines.iter().find_map(|line| {
        if env_line_key(line)? != key {
            return None;
        }
        let raw = line.split_once('=')?.1.trim();
        let value = raw.strip_prefix("export ").unwrap_or(raw);
        let value = value.trim().trim_matches('"').trim_matches('\'');
        if value.is_empty() {
            None
        } else {
            Some(value.to_string())
        }
    })
}

fn env_line_key(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let trimmed = trimmed.strip_prefix("export ").unwrap_or(trimmed).trim_start();
    let (key, _) = trimmed.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        None
    } else {
        Some(key)
    }
}

fn split_lines(text: &str) -> (Vec<String>, &'static str) {
    let newline = if text.contains("\r\n") { "\r\n" } else { "\n" };
    if text.is_empty() {
        return (Vec::new(), newline);
    }
    let mut lines: Vec<String> = text.split(newline).map(str::to_string).collect();
    if text.ends_with(newline) {
        lines.pop();
    }
    (lines, newline)
}

fn join_lines(lines: &[String], newline: &str) -> String {
    let mut text = lines.join(newline);
    if !text.is_empty() && !text.ends_with(newline) {
        text.push_str(newline);
    }
    text
}

fn ensure_trailing_newline(text: String, newline: &str) -> String {
    if text.is_empty() || text.ends_with(newline) {
        text
    } else {
        format!("{text}{newline}")
    }
}

fn top_scope_end(lines: &[String]) -> usize {
    lines
        .iter()
        .position(|line| header_name(line).is_some())
        .unwrap_or(lines.len())
}

fn header_name(line: &str) -> Option<&str> {
    let trimmed = line.trim();
    if trimmed.starts_with('#') {
        return None;
    }
    let rest = trimmed.strip_prefix('[')?;
    let end = rest.find(']')?;
    let name = rest[..end].trim();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn is_our_section(name: &str) -> bool {
    name == format!("model_providers.{PROVIDER_ID}")
        || name.starts_with(&format!("model_providers.{PROVIDER_ID}."))
}

fn is_top_key(line: &str, key: &str) -> bool {
    let trimmed = line.trim_start();
    if trimmed.starts_with('#') {
        return false;
    }
    trimmed
        .strip_prefix(key)
        .is_some_and(|rest| rest.trim_start().starts_with('='))
}

fn assignment_value(line: &str) -> Option<String> {
    let trimmed = line.trim();
    let (_, raw) = trimmed.split_once('=')?;
    let raw = raw.trim();
    if let Some(inner) = raw.strip_prefix('"') {
        let mut out = String::new();
        let mut chars = inner.chars();
        while let Some(ch) = chars.next() {
            if ch == '\\' {
                out.push(chars.next().unwrap_or('\\'));
                continue;
            }
            if ch == '"' {
                return Some(out);
            }
            out.push(ch);
        }
        return None;
    }
    if let Some(inner) = raw.strip_prefix('\'') {
        let end = inner.find('\'')?;
        return Some(inner[..end].to_string());
    }
    Some(raw.split_whitespace().next().unwrap_or("").to_string())
}

fn config_path(codex_home: &Path) -> PathBuf {
    codex_home.join("config.toml")
}

fn env_path(codex_home: &Path) -> PathBuf {
    codex_home.join(".env")
}

fn read_to_string_if_utf8(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    String::from_utf8(bytes).ok()
}

fn replace_file(path: &Path, bytes: &[u8], mode: u32) -> Result<(), GatewayError> {
    if let Some(parent) = path.parent() {
        let created = !parent.exists();
        fs::create_dir_all(parent).map_err(io_error)?;
        if created {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
            }
        }
    }
    let tmp = temp_path(path);
    let bak = backup_path(path);
    {
        let mut file = File::create(&tmp).map_err(io_error)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(mode))
                .map_err(io_error)?;
        }
        let _ = mode;
        file.write_all(bytes).map_err(io_error)?;
        file.sync_all().map_err(io_error)?;
    }
    if path.exists() {
        let _ = fs::remove_file(&bak);
        fs::rename(path, &bak).map_err(io_error)?;
    }
    if let Err(error) = fs::rename(&tmp, path) {
        if !path.exists() && bak.exists() {
            let _ = fs::rename(&bak, path);
        }
        let _ = fs::remove_file(&tmp);
        return Err(io_error(error));
    }
    let _ = fs::remove_file(&bak);
    let _ = fs::remove_file(&tmp);
    Ok(())
}

fn temp_path(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    path.with_file_name(format!(".{name}.{}.tmp", std::process::id()))
}

fn backup_path(path: &Path) -> PathBuf {
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
    path.with_file_name(format!("{name}.bak"))
}

fn io_error(error: io::Error) -> GatewayError {
    GatewayError(format!("写入 Codex 配置失败: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "cam-gateway-{name}-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4()
        ));
        let _ = fs::remove_dir_all(&path);
        path
    }

    #[test]
    fn writes_provider_and_keeps_the_key_out_of_config() {
        let dir = home("fresh");
        apply_gateway(
            &dir,
            &CodexGatewayInput {
                base_url: "https://aiapi.yxrobot.com/v1/".into(),
                model: "gpt-5.4".into(),
                api_key: "sk-test-key".into(),
            },
        )
        .unwrap();

        let config = fs::read_to_string(config_path(&dir)).unwrap();
        let env = fs::read_to_string(env_path(&dir)).unwrap();
        assert!(config.contains("model = \"gpt-5.4\""));
        assert!(config.contains("model_provider = \"yxrobot\""));
        assert!(config.contains("base_url = \"https://aiapi.yxrobot.com/v1\""));
        assert!(config.contains("env_key = \"YXROBOT_API_KEY\""));
        assert!(config.contains("wire_api = \"responses\""));
        assert!(!config.contains("sk-test-key"));
        assert!(env.contains("YXROBOT_API_KEY=sk-test-key"));

        let status = read_gateway(&dir);
        assert_eq!(status.base_url, "https://aiapi.yxrobot.com/v1");
        assert_eq!(status.model, "gpt-5.4");
        assert!(status.api_key_set);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn preserves_unrelated_config_and_other_env_vars() {
        let dir = home("preserve");
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            config_path(&dir),
            "appearanceTheme = \"dark\"\n\n[desktop.appearanceDarkChromeTheme]\naccent = \"#111111\"\n",
        )
        .unwrap();
        fs::write(env_path(&dir), "OTHER_TOKEN=keep-me\n").unwrap();

        apply_gateway(
            &dir,
            &CodexGatewayInput {
                base_url: "https://aiapi.yxrobot.com/v1".into(),
                model: "internal-model".into(),
                api_key: "sk-one".into(),
            },
        )
        .unwrap();

        let config = fs::read_to_string(config_path(&dir)).unwrap();
        let env = fs::read_to_string(env_path(&dir)).unwrap();
        assert!(config.contains("appearanceTheme = \"dark\""));
        assert!(config.contains("accent = \"#111111\""));
        assert!(env.contains("OTHER_TOKEN=keep-me"));
        assert!(env.contains("YXROBOT_API_KEY=sk-one"));

        apply_gateway(
            &dir,
            &CodexGatewayInput {
                base_url: "https://aiapi.yxrobot.com/v1".into(),
                model: "internal-model-2".into(),
                api_key: String::new(),
            },
        )
        .unwrap();
        let env = fs::read_to_string(env_path(&dir)).unwrap();
        assert!(env.contains("YXROBOT_API_KEY=sk-one"));
        assert!(fs::read_to_string(config_path(&dir))
            .unwrap()
            .contains("model = \"internal-model-2\""));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_model_ids_from_a_gateway_payload() {
        let ids = parse_model_ids(
            r#"{"data":[{"id":"openai/gpt-5.4"},{"id":"openai/gpt-5.4"},{"id":"  "},{"id":"claude-sonnet"}]}"#,
        )
        .unwrap();
        assert_eq!(ids, vec!["claude-sonnet".to_string(), "openai/gpt-5.4".to_string()]);
    }

    #[test]
    fn rejects_http_empty_model_and_a_missing_key() {
        let dir = home("reject");
        let bad_url = apply_gateway(
            &dir,
            &CodexGatewayInput {
                base_url: "http://aiapi.yxrobot.com/v1".into(),
                model: "gpt-5.4".into(),
                api_key: "sk-test".into(),
            },
        );
        assert!(bad_url.is_err());
        let bad_model = apply_gateway(
            &dir,
            &CodexGatewayInput {
                base_url: "https://aiapi.yxrobot.com/v1".into(),
                model: "bad model".into(),
                api_key: "sk-test".into(),
            },
        );
        assert!(bad_model.is_err());
        let missing_key = apply_gateway(
            &dir,
            &CodexGatewayInput {
                base_url: "https://aiapi.yxrobot.com/v1".into(),
                model: "gpt-5.4".into(),
                api_key: String::new(),
            },
        );
        assert!(missing_key.is_err());
        assert!(!config_path(&dir).exists());
        let _ = fs::remove_dir_all(&dir);
    }
}
