//! Link previews (Open Graph): the first bytes of a page are fetched with curl and its `og:` / `twitter:` /
//! `<title>` data read, so a message with a link can show an image, a title and a description.
//!
//! The page is asked for by this computer, so the site learns the link was seen: the window only does it when
//! `[links] previews = "all"` (the default). Addresses that point inside the machine or the local network are refused.

use serde_json::{Value, json};
use std::io::Read;
use std::net::{IpAddr, ToSocketAddrs};
use std::process::{Command, Stdio};

const MAX_BYTES: u64 = 400_000;

/// What the page says about itself; empty fields are left out.
pub fn parse(html: &str, page_url: &str) -> Value {
    let metas = metas(html);
    let pick = |keys: &[&str]| {
        keys.iter().find_map(|k| metas.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())).filter(|v| !v.trim().is_empty())
    };
    let title = pick(&["og:title", "twitter:title"]).or_else(|| title_tag(html));
    let description = pick(&["og:description", "twitter:description", "description"]);
    let image = pick(&["og:image", "og:image:url", "twitter:image", "twitter:image:src"]).map(|i| absolute(&i, page_url));
    let site = pick(&["og:site_name"]).or_else(|| host_of(page_url));
    let mut out = serde_json::Map::new();
    out.insert("url".into(), json!(page_url));
    for (k, v) in [("title", title), ("description", description), ("image", image), ("site", site)] {
        if let Some(v) = v {
            out.insert(k.into(), json!(v.split_whitespace().collect::<Vec<_>>().join(" ")));
        }
    }
    Value::Object(out)
}

/// (property or name, content) of every `<meta>` tag, names lowercased, entities decoded.
fn metas(html: &str) -> Vec<(String, String)> {
    let lower = html.to_ascii_lowercase();
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(i) = lower[at..].find("<meta") {
        let start = at + i;
        let Some(len) = lower[start..].find('>') else { break };
        let tag = &html[start..start + len];
        at = start + len;
        let name = attr(tag, "property").or_else(|| attr(tag, "name"));
        if let (Some(name), Some(content)) = (name, attr(tag, "content")) {
            out.push((name.to_ascii_lowercase(), decode(&content)));
        }
    }
    out
}

/// The value of an attribute (quoted with " or ', or bare) of a tag.
fn attr(tag: &str, name: &str) -> Option<String> {
    let lower = tag.to_ascii_lowercase();
    let mut from = 0;
    while let Some(i) = lower[from..].find(name) {
        let pos = from + i;
        from = pos + name.len();
        let before_ok = pos == 0
            || lower.as_bytes()[pos - 1].is_ascii_whitespace()
            || lower.as_bytes()[pos - 1] == b'"'
            || lower.as_bytes()[pos - 1] == b'\'';
        let rest = tag[pos + name.len()..].trim_start();
        let Some(rest) = rest.strip_prefix('=') else { continue };
        if !before_ok {
            continue;
        }
        let rest = rest.trim_start();
        return Some(match rest.chars().next()? {
            q @ ('"' | '\'') => rest[1..].split(q).next()?.to_string(),
            _ => rest.split(|c: char| c.is_whitespace() || c == '/' || c == '>').next()?.to_string(),
        });
    }
    None
}

fn title_tag(html: &str) -> Option<String> {
    let lower = html.to_ascii_lowercase();
    let start = lower.find("<title")?;
    let open_end = start + lower[start..].find('>')? + 1;
    let end = open_end + lower[open_end..].find("</title")?;
    Some(decode(&html[open_end..end])).filter(|t| !t.trim().is_empty())
}

pub fn decode(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let Some(end) = rest[..rest.len().min(12)].find(';') else {
            out.push('&');
            rest = &rest[1..];
            continue;
        };
        let entity = &rest[1..end];
        let ch = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            "nbsp" => Some(' '),
            _ => entity
                .strip_prefix("#x")
                .or_else(|| entity.strip_prefix("#X"))
                .and_then(|h| u32::from_str_radix(h, 16).ok())
                .or_else(|| entity.strip_prefix('#').and_then(|d| d.parse().ok()))
                .and_then(char::from_u32),
        };
        match ch {
            Some(c) => {
                out.push(c);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let host = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    let host = host.rsplit_once(':').filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit())).map_or(host, |(h, _)| h);
    Some(host.trim_start_matches("www.").to_string()).filter(|h| !h.is_empty())
}

/// An image address from the page, made absolute against the page.
fn absolute(link: &str, page: &str) -> String {
    if link.starts_with("http://") || link.starts_with("https://") {
        return link.to_string();
    }
    let Some((scheme, rest)) = page.split_once("://") else { return link.to_string() };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    if let Some(l) = link.strip_prefix("//") {
        return format!("{scheme}://{l}");
    }
    if link.starts_with('/') {
        return format!("{scheme}://{host}{link}");
    }
    let dir = rest.split(['?', '#']).next().unwrap_or("").rsplit_once('/').map_or(host, |(d, _)| d);
    format!("{scheme}://{dir}/{link}")
}

/// Addresses that lead into this machine or its network are never fetched.
fn is_private(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            a.is_private()
                || a.is_loopback()
                || a.is_link_local()
                || a.is_unspecified()
                || a.is_broadcast()
                || a.octets()[0] == 0
                || (a.octets()[0] == 100 && (64..128).contains(&a.octets()[1]))
        }
        IpAddr::V6(a) => {
            a.is_loopback()
                || a.is_unspecified()
                || (a.segments()[0] & 0xfe00) == 0xfc00
                || (a.segments()[0] & 0xffc0) == 0xfe80
                || a.to_ipv4_mapped().is_some_and(|v4| is_private(IpAddr::V4(v4)))
        }
    }
}

/// What yt-dlp found for a video page, reduced to what the player needs: `{title, duration, stream}`, `stream` being one
/// address that carries picture and sound (a muxed file, or the HLS master playlist that holds both parts).
pub fn video_from_ytdlp(info: &Value) -> Value {
    let mut out = serde_json::Map::new();
    for k in ["title", "duration"] {
        if let Some(v) = info.get(k).filter(|v| !v.is_null()) {
            out.insert(k.into(), v.clone());
        }
    }
    let stream = match info.get("requested_formats").and_then(Value::as_array) {
        // Picture and sound as two files cannot be played together; as two parts of one master playlist they can
        Some(parts) => {
            let masters: Vec<&str> = parts.iter().filter_map(|p| p.get("manifest_url").and_then(Value::as_str)).collect();
            (masters.len() == parts.len() && masters.windows(2).all(|w| w[0] == w[1])).then(|| masters[0].to_string())
        }
        None => info.get("url").and_then(Value::as_str).map(str::to_owned),
    };
    match stream {
        Some(s) => out.insert("stream".into(), json!(s)),
        None => out.insert("error".into(), json!("no single stream")),
    };
    Value::Object(out)
}

/// The master playlist of an HLS stream reduced to one variant: the best H.264 one up to `max_height` (a player left
/// alone starts with the smallest), its default audio, and no subtitles. `None` when it has no variant.
pub fn pick_variant(master: &str, max_height: u32) -> Option<String> {
    let attr = |line: &str, name: &str| -> Option<String> {
        let at = line.find(&format!("{name}="))? + name.len() + 1;
        let rest = &line[at..];
        Some(match rest.strip_prefix('"') {
            Some(q) => q.split('"').next()?.to_string(),
            None => rest.split(',').next()?.to_string(),
        })
    };
    let lines: Vec<&str> = master.lines().map(str::trim).collect();
    let mut best: Option<(bool, u32, u64, &str, &str)> = None; // (h264 within limit, height, bandwidth, inf line, uri)
    for (i, line) in lines.iter().enumerate() {
        if !line.starts_with("#EXT-X-STREAM-INF") {
            continue;
        }
        let Some(uri) = lines.get(i + 1).filter(|u| !u.starts_with('#') && !u.is_empty()) else { continue };
        let height = attr(line, "RESOLUTION").and_then(|r| r.split('x').nth(1)?.parse::<u32>().ok()).unwrap_or(0);
        let bandwidth = attr(line, "BANDWIDTH").and_then(|b| b.parse::<u64>().ok()).unwrap_or(0);
        let h264 = attr(line, "CODECS").is_some_and(|c| c.contains("avc1"));
        let within = height <= max_height;
        // Rank: within the limit and H.264 first, then the taller, then the heavier (the smallest when none fits)
        let rank = (within && h264, if within { height } else { u32::MAX - height }, bandwidth);
        if best.as_ref().is_none_or(|b| rank > (b.0, b.1, b.2)) {
            best = Some((rank.0, rank.1, rank.2, line, uri));
        }
    }
    let (_, _, _, inf, uri) = best?;
    let group = attr(inf, "AUDIO");
    let mut out = String::from("#EXTM3U\n#EXT-X-INDEPENDENT-SEGMENTS\n");
    if let Some(group) = &group {
        let audio: Vec<&&str> = lines
            .iter()
            .filter(|l| {
                l.starts_with("#EXT-X-MEDIA")
                    && attr(l, "TYPE").as_deref() == Some("AUDIO")
                    && attr(l, "GROUP-ID").as_ref() == Some(group)
            })
            .collect();
        let chosen = audio.iter().find(|l| attr(l, "DEFAULT").as_deref() == Some("YES")).or(audio.first());
        if let Some(l) = chosen {
            out.push_str(l);
            out.push('\n');
        }
    }
    // The variant line without its subtitles
    let mut inf = inf.to_string();
    if let Some(s) = inf.find("SUBTITLES=\"") {
        let end = inf[s + 11..].find('"').map_or(inf.len(), |e| s + 11 + e + 1);
        inf.replace_range(s..end, "");
        inf = inf.replace(",,", ",").replace(",\n", "\n");
        if inf.ends_with(',') {
            inf.pop();
        }
    }
    out.push_str(&inf);
    out.push('\n');
    out.push_str(uri);
    out.push('\n');
    Some(out)
}

/// Playlists served to the player on 127.0.0.1 (one unguessable path each).
fn serve_playlist(body: String) -> Option<String> {
    use std::collections::HashMap;
    use std::io::Write;
    use std::sync::{Arc, Mutex, OnceLock};
    type Table = Arc<Mutex<HashMap<String, String>>>;
    static SERVER: OnceLock<(u16, Table)> = OnceLock::new();
    let (port, table) = SERVER.get_or_init(|| {
        let table: Table = Arc::default();
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("a free local port");
        let port = listener.local_addr().map(|a| a.port()).unwrap_or(0);
        let shared = table.clone();
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                let shared = shared.clone();
                std::thread::spawn(move || {
                    let mut stream = stream;
                    let mut buf = [0u8; 2048];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    let request = String::from_utf8_lossy(&buf[..n]);
                    let path = request.lines().next().and_then(|l| l.split_whitespace().nth(1)).unwrap_or("/").trim_start_matches('/').to_string();
                    let body = shared.lock().unwrap().get(&path).cloned();
                    let reply = match body {
                        Some(b) => format!("HTTP/1.1 200 OK\r\nContent-Type: application/vnd.apple.mpegurl\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{b}", b.len()),
                        None => "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_string(),
                    };
                    let _ = stream.write_all(reply.as_bytes());
                });
            }
        });
        (port, table)
    });
    let mut raw = [0u8; 12];
    std::fs::File::open("/dev/urandom").ok()?.read_exact(&mut raw).ok()?;
    let token: String = raw.iter().map(|b| format!("{b:02x}")).collect::<String>() + ".m3u8";
    table.lock().unwrap().insert(token.clone(), body);
    Some(format!("http://127.0.0.1:{port}/{token}"))
}

/// A stream address for the player: an HLS master playlist is narrowed to one good variant and served locally.
fn playable(stream: &str) -> String {
    if !stream.contains(".m3u8") && !stream.contains("/hls_variant/") && !stream.contains("manifest") {
        return stream.to_string();
    }
    let master = Command::new("curl")
        .args(["-sL", "--max-time", "15", "--compressed", "--"])
        .arg(stream)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    master
        .filter(|m| m.contains("#EXT-X-STREAM-INF"))
        .and_then(|m| pick_variant(&m, 720))
        .and_then(serve_playlist)
        .unwrap_or_else(|| stream.to_string())
}

/// Ask yt-dlp about a video page (YouTube, Instagram, TikTok, Vimeo… whatever it knows).
pub fn resolve_video(url: &str) -> Value {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        return json!({"error": "not a web address"});
    }
    let run = Command::new("yt-dlp")
        .args(["--no-playlist", "--no-warnings", "--socket-timeout", "15", "-j"])
        .args(["-f", "b[protocol^=https][height<=720]/bv*[protocol^=m3u8][height<=720]+ba[protocol^=m3u8]/b[height<=720]/b"])
        .arg("--")
        .arg(url)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    match run {
        Ok(out) if out.status.success() => match serde_json::from_slice::<Value>(&out.stdout) {
            Ok(info) => {
                let mut v = video_from_ytdlp(&info);
                if let Some(stream) = v.get("stream").and_then(Value::as_str).map(playable) {
                    v["stream"] = json!(stream);
                }
                v
            }
            Err(_) => json!({"error": "unreadable answer"}),
        },
        Ok(_) => json!({"error": "this video cannot be read"}),
        Err(_) => json!({"error": "yt-dlp is not installed"}),
    }
}

/// Fetch a page's start and read it. `None`: not a web address, a private one, unreachable or not a page.
pub fn fetch(url: &str) -> Option<Value> {
    let (scheme, rest) = url.split_once("://")?;
    if scheme != "http" && scheme != "https" {
        return None;
    }
    let authority = rest.split(['/', '?', '#']).next()?.rsplit('@').next()?;
    let (host, port) = match authority.rsplit_once(':').filter(|(_, p)| p.chars().all(|c| c.is_ascii_digit()) && !p.is_empty()) {
        Some((h, p)) => (h.trim_matches(['[', ']']), p.parse::<u16>().ok()?),
        None => (authority.trim_matches(['[', ']']), if scheme == "https" { 443 } else { 80 }),
    };
    if host.is_empty() || host.eq_ignore_ascii_case("localhost") || host.ends_with(".local") || host.ends_with(".internal") {
        return None;
    }
    // Resolve here and pin the address for curl: the answer checked is the one connected to
    let addrs: Vec<std::net::SocketAddr> = (host, port).to_socket_addrs().ok()?.collect();
    let ip = addrs.iter().map(|a| a.ip()).find(|ip| !is_private(*ip))?;
    if addrs.iter().any(|a| is_private(a.ip())) {
        return None;
    }
    let mut child = Command::new("curl")
        .args([
            "-sL",
            "--max-time",
            "8",
            "--max-redirs",
            "3",
            "--proto",
            "=http,https",
            "--proto-redir",
            "=http,https",
            "--compressed",
        ])
        .args(["-A", "Mozilla/5.0 (compatible; Conversations link preview)", "-H", "Accept: text/html,application/xhtml+xml"])
        .args(["--resolve", &format!("{host}:{port}:{ip}")])
        .arg("--")
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut bytes = Vec::new();
    let _ = child.stdout.take()?.take(MAX_BYTES).read_to_end(&mut bytes);
    let _ = child.kill();
    let _ = child.wait();
    let html = String::from_utf8_lossy(&bytes);
    if !html.contains('<') {
        return None;
    }
    let found = parse(&html, url);
    (found.get("title").is_some() || found.get("image").is_some()).then_some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_graph_data_is_read_in_either_attribute_order() {
        let html = r#"<html><head><title>Fallback &amp; co</title>
            <meta property="og:title" content="Une &quot;belle&quot; page">
            <meta content='Le résumé &#233; de la page' property='og:description'>
            <meta property="og:image" content="/img/cover.jpg">
            <meta property="og:site_name" content="Le Site"></head></html>"#;
        let v = parse(html, "https://example.org/articles/1?x=1");
        assert_eq!(v["title"], "Une \"belle\" page");
        assert_eq!(v["description"], "Le résumé é de la page");
        assert_eq!(v["image"], "https://example.org/img/cover.jpg");
        assert_eq!(v["site"], "Le Site");
    }

    #[test]
    fn a_stream_is_one_address_with_picture_and_sound() {
        let muxed = json!({"title": "t", "duration": 5, "url": "https://x/v.mp4"});
        assert_eq!(video_from_ytdlp(&muxed)["stream"], "https://x/v.mp4");
        let hls =
            json!({"requested_formats": [{"manifest_url": "https://x/master.m3u8"}, {"manifest_url": "https://x/master.m3u8"}]});
        assert_eq!(video_from_ytdlp(&hls)["stream"], "https://x/master.m3u8");
        let split = json!({"requested_formats": [{"url": "https://x/v.mp4"}, {"url": "https://x/a.m4a"}]});
        assert!(video_from_ytdlp(&split).get("stream").is_none(), "two files cannot be played together");
    }

    #[test]
    fn a_master_playlist_is_narrowed_to_one_good_variant_with_its_audio() {
        let master = "#EXTM3U\n#EXT-X-MEDIA:URI=\"https://a/233.m3u8\",TYPE=AUDIO,GROUP-ID=\"233\",NAME=\"Default\",DEFAULT=YES\n\
#EXT-X-MEDIA:URI=\"https://a/234.m3u8\",TYPE=AUDIO,GROUP-ID=\"234\",NAME=\"Default\",DEFAULT=YES\n\
#EXT-X-MEDIA:URI=\"https://s/en.m3u8\",TYPE=SUBTITLES,GROUP-ID=\"vtt\",NAME=\"English\"\n\
#EXT-X-STREAM-INF:BANDWIDTH=300000,CODECS=\"avc1.4D4015,mp4a.40.5\",RESOLUTION=426x240,AUDIO=\"233\",SUBTITLES=\"vtt\",CLOSED-CAPTIONS=NONE\nhttps://v/240.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=1248435,CODECS=\"avc1.4D401F,mp4a.40.2\",RESOLUTION=1280x720,AUDIO=\"234\",SUBTITLES=\"vtt\",CLOSED-CAPTIONS=NONE\nhttps://v/720.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=1048474,CODECS=\"vp09.00.31.08,mp4a.40.2\",RESOLUTION=1280x720,AUDIO=\"234\",SUBTITLES=\"vtt\"\nhttps://v/720vp9.m3u8\n\
#EXT-X-STREAM-INF:BANDWIDTH=4688074,CODECS=\"avc1.640028,mp4a.40.2\",RESOLUTION=1920x1080,AUDIO=\"234\",SUBTITLES=\"vtt\"\nhttps://v/1080.m3u8\n";
        let one = pick_variant(master, 720).unwrap();
        assert!(one.contains("https://v/720.m3u8"), "{one}");
        assert!(!one.contains("1080") && !one.contains("240") && !one.contains("vp9"), "{one}");
        assert!(one.contains("GROUP-ID=\"234\"") && !one.contains("GROUP-ID=\"233\""), "{one}");
        assert!(!one.contains("SUBTITLES") && !one.contains("vtt"), "{one}");
        assert!(one.contains("AUDIO=\"234\""));
        // Nothing within the limit: the smallest
        assert!(pick_variant(master, 100).unwrap().contains("https://v/240.m3u8"));
    }

    #[test]
    fn the_title_tag_and_the_host_fill_in_when_there_is_no_open_graph() {
        let v = parse("<title>  Hello\n world </title><meta name=\"description\" content=\"d\">", "https://www.example.org/");
        assert_eq!(v["title"], "Hello world");
        assert_eq!(v["description"], "d");
        assert_eq!(v["site"], "example.org");
        assert!(v.get("image").is_none());
    }

    #[test]
    fn addresses_inside_the_machine_or_the_network_are_refused() {
        for url in [
            "http://localhost/x",
            "http://127.0.0.1/",
            "http://192.168.1.1/",
            "http://10.0.0.5:8080/",
            "http://[::1]/",
            "http://169.254.169.254/latest",
            "ftp://example.org/",
            "file:///etc/passwd",
            "http://printer.local/",
        ] {
            assert!(fetch(url).is_none(), "{url}");
        }
    }
}
