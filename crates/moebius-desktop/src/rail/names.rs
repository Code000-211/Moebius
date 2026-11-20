//! What the rail calls a server: the name the server reports of the computer it runs on, or else its address without
//! the scheme, and, when two servers would have the same name, the name with the address that tells them apart.

/// An address as the rail writes it: no scheme, no `www.`, no `/` at the end, and at most 28 characters (a long one
/// keeps its start and its end, where the port is).
pub fn short_address(address: &str) -> String {
    let text = address.trim().trim_end_matches('/');
    let text = text.split_once("://").map_or(text, |(_, rest)| rest);
    let text = text.strip_prefix("www.").unwrap_or(text);
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= 28 {
        return text.to_string();
    }
    let start: String = chars[..12].iter().collect();
    let end: String = chars[chars.len() - 15..].iter().collect();
    format!("{start}…{end}")
}

/// The name a server gets when it is added: the host it reports (`GET /v1/server`), if it reports one, else its short
/// address.
pub fn default_name(reported: Option<&str>, address: &str) -> String {
    match reported.map(str::trim) {
        Some(host) if !host.is_empty() => host.chars().take(40).collect(),
        _ => short_address(address),
    }
}

/// The names the rail shows for servers with these `(name, address)` pairs, in the same order: a name that two
/// servers share is followed by the address of each, so that no two squares carry the same name.
pub fn distinct(servers: &[(&str, &str)]) -> Vec<String> {
    servers
        .iter()
        .map(|(name, address)| {
            let shared = servers.iter().filter(|(other, _)| other == name).count() > 1;
            let address = short_address(address);
            if shared && address != *name {
                format!("{name} · {address}")
            } else {
                (*name).to_string()
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_address_loses_its_scheme_and_a_long_one_its_middle() {
        assert_eq!(short_address("http://127.0.0.1:7171/"), "127.0.0.1:7171");
        assert_eq!(short_address("https://www.example.org"), "example.org");
        let long = short_address("http://a-very-long-host-name.internal.example.org:7171");
        assert_eq!(long.chars().count(), 28);
        assert!(long.ends_with(":7171") && long.starts_with("a-very-long-"));
    }

    #[test]
    fn the_reported_host_names_a_server_and_the_address_does_when_there_is_none() {
        assert_eq!(
            default_name(Some(" laptop "), "http://10.0.0.5:7171"),
            "laptop"
        );
        assert_eq!(
            default_name(Some(""), "http://10.0.0.5:7171"),
            "10.0.0.5:7171"
        );
        assert_eq!(default_name(None, "http://10.0.0.5:7171"), "10.0.0.5:7171");
    }

    #[test]
    fn servers_with_the_same_name_are_told_apart_by_their_addresses() {
        let names = distinct(&[
            ("laptop", "http://127.0.0.1:7171"),
            ("laptop", "http://127.0.0.1:7172"),
            ("build", "http://10.0.0.5:7171"),
            ("10.0.0.6:7171", "http://10.0.0.6:7171"),
        ]);
        assert_eq!(
            names,
            [
                "laptop · 127.0.0.1:7171",
                "laptop · 127.0.0.1:7172",
                "build",
                "10.0.0.6:7171"
            ]
        );
    }
}
