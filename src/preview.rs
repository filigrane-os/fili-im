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
