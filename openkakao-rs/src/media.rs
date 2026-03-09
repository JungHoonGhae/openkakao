use std::path::Path;

use anyhow::Result;

use crate::model::KakaoCredentials;

/// Detect media type from magic bytes, falling back to file extension.
/// Returns (kakao_msg_type, extension).
pub fn detect_media_type(data: &[u8], file_ext: &str) -> (i32, String) {
    // Magic bytes detection
    if data.len() >= 2 && data[0] == 0xFF && data[1] == 0xD8 {
        return (2, "jpg".into());
    }
    if data.len() >= 8 && &data[..8] == b"\x89PNG\r\n\x1a\n" {
        return (2, "png".into());
    }
    if data.len() >= 4 && &data[..4] == b"GIF8" {
        return (14, "gif".into());
    }
    // Video: ftyp box (MP4/MOV/3GP)
    if data.len() >= 8 && &data[4..8] == b"ftyp" {
        return (3, if file_ext == "mov" { "mov" } else { "mp4" }.into());
    }
    // WebM
    if data.len() >= 4 && &data[..4] == b"\x1a\x45\xdf\xa3" {
        return (3, "webm".into());
    }

    // Fall back to extension
    match file_ext {
        "jpg" | "jpeg" => (2, "jpg".into()),
        "png" => (2, "png".into()),
        "gif" => (14, "gif".into()),
        "mp4" | "mov" | "avi" | "mkv" | "webm" => (3, file_ext.into()),
        "m4a" | "aac" | "mp3" | "wav" | "ogg" => (12, file_ext.into()),
        _ => (
            26,
            if file_ext.is_empty() { "bin" } else { file_ext }.into(),
        ),
    }
}

/// Extract JPEG dimensions from SOF marker.
pub fn jpeg_dimensions(data: &[u8]) -> Option<(i32, i32)> {
    if data.len() < 4 || data[0] != 0xFF || data[1] != 0xD8 {
        return None;
    }
    let mut i = 2;
    while i + 1 < data.len() {
        if data[i] != 0xFF {
            i += 1;
            continue;
        }
        let marker = data[i + 1];
        i += 2;
        if i + 2 > data.len() {
            return None;
        }
        // SOF markers (C0-CF except C4, C8, CC)
        if (0xC0..=0xCF).contains(&marker) && marker != 0xC4 && marker != 0xC8 && marker != 0xCC {
            if i + 7 > data.len() {
                return None;
            }
            let height = ((data[i + 3] as i32) << 8) | (data[i + 4] as i32);
            let width = ((data[i + 5] as i32) << 8) | (data[i + 6] as i32);
            return Some((width, height));
        }
        let len = ((data[i] as usize) << 8) | (data[i + 1] as usize);
        if len < 2 {
            return None;
        }
        i += len;
    }
    None
}

/// Extract PNG dimensions from IHDR chunk.
pub fn png_dimensions(data: &[u8]) -> Option<(i32, i32)> {
    if data.len() < 24 || &data[..8] != b"\x89PNG\r\n\x1a\n" {
        return None;
    }
    let width = ((data[16] as i32) << 24)
        | ((data[17] as i32) << 16)
        | ((data[18] as i32) << 8)
        | (data[19] as i32);
    let height = ((data[20] as i32) << 24)
        | ((data[21] as i32) << 16)
        | ((data[22] as i32) << 8)
        | (data[23] as i32);
    Some((width, height))
}

/// Parse attachment JSON to extract download URL and filename.
/// Returns (url, filename) or None if unparseable.
pub fn parse_attachment_url(attachment: &str, msg_type: i32) -> Option<(String, String)> {
    let v: serde_json::Value = serde_json::from_str(attachment).ok()?;

    // Try direct "url" field first
    if let Some(url) = v.get("url").and_then(|u| u.as_str()) {
        if !url.is_empty() {
            let filename = v
                .get("name")
                .and_then(|n| n.as_str())
                .filter(|n| !n.is_empty() && *n != "(Emoticons)")
                .map(String::from)
                .or_else(|| {
                    // Try to extract filename from "k" field
                    v.get("k")
                        .and_then(|k| k.as_str())
                        .and_then(|k| k.rsplit('/').next())
                        .filter(|n| n.contains('.'))
                        .map(String::from)
                })
                .unwrap_or_else(|| {
                    let ext = media_extension(msg_type);
                    format!("media.{}", ext)
                });
            return Some((url.to_string(), filename));
        }
    }

    // Try "k" field (photo/video key): https://dn-m.talk.kakao.com/talkm/{k}
    if let Some(k) = v.get("k").and_then(|k| k.as_str()) {
        if !k.is_empty() {
            let url = format!("https://dn-m.talk.kakao.com/talkm/{}", k);
            // Use the key's last segment as filename base
            let key_name = k.rsplit('/').next().unwrap_or(k);
            let ext = media_extension(msg_type);
            let filename = if key_name.contains('.') {
                key_name.to_string()
            } else {
                format!("{}.{}", key_name, ext)
            };
            return Some((url, filename));
        }
    }

    None
}

pub fn media_extension(msg_type: i32) -> &'static str {
    match msg_type {
        2 | 27 => "jpg",
        3 => "mp4",
        12 => "m4a",
        14 => "gif",
        26 => "bin",
        _ => "dat",
    }
}

/// Sanitize a filename by stripping path components and dangerous characters.
pub fn sanitize_filename(name: &str) -> String {
    let base = Path::new(name)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("download");
    let sanitized: String = base
        .chars()
        .filter(|c| *c != '\0' && *c != '/' && *c != '\\')
        .collect();
    if sanitized.is_empty() || sanitized == "." || sanitized == ".." {
        "download".to_string()
    } else {
        sanitized
    }
}

/// Download a media file from KakaoTalk CDN.
pub fn download_media_file(creds: &KakaoCredentials, url: &str, path: &Path) -> Result<u64> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let a_header = if creds.a_header.is_empty() {
        format!("mac/{}/ko", creds.app_version)
    } else {
        creds.a_header.clone()
    };
    let user_agent = if creds.user_agent.is_empty() {
        format!("KT/{} Mc/10.15.7 ko", creds.app_version)
    } else {
        creds.user_agent.clone()
    };

    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(60))
        .build()?;

    // Validate URL domain before sending credentials
    let parsed_url = reqwest::Url::parse(url)?;
    let host = parsed_url.host_str().unwrap_or("");
    if !host.ends_with(".kakao.com") && !host.ends_with(".kakaocdn.net") {
        anyhow::bail!("Refusing to send credentials to non-Kakao domain: {}", host);
    }

    let mut response = client
        .get(url)
        .header("A", &a_header)
        .header("User-Agent", &user_agent)
        .header(
            "Authorization",
            format!("{}-{}", creds.oauth_token, creds.device_uuid),
        )
        .send()?;

    if !response.status().is_success() {
        anyhow::bail!("HTTP {}: {}", response.status(), url);
    }

    let mut file = std::fs::File::create(path)?;
    let bytes = std::io::copy(&mut response, &mut file)?;
    Ok(bytes)
}
