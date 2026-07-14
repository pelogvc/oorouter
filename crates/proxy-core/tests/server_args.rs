use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use proxy_core::server_args::{parse_server_args, ServerArgsError};

fn api_key(character: char) -> String {
    format!("sk-{}", character.to_string().repeat(64))
}

fn output_with_timeout(mut command: Command, timeout: Duration) -> Output {
    let mut child = command
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn standalone server");
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().expect("poll standalone server").is_some() {
            return child.wait_with_output().expect("collect process output");
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait_with_output().expect("collect timed-out output");
            panic!("standalone server did not reject invalid arguments before timeout");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn no_arguments_keep_loopback_and_client_auth_disabled() {
    let parsed = parse_server_args(std::iter::empty::<&str>()).expect("parse no arguments");

    assert_eq!(parsed.host, IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert!(parsed.api_keys.is_empty());
    assert!(!parsed.show_help);
}

#[test]
fn repeated_api_keys_are_validated_and_deduplicated() {
    let first = format!("sk-{}", "Aa0Zz9bB".repeat(8));
    let second = api_key('B');
    let parsed = parse_server_args([
        "--api-key",
        first.as_str(),
        "--api-key",
        second.as_str(),
        "--api-key",
        first.as_str(),
    ])
    .expect("parse repeated API keys");

    assert_eq!(parsed.api_keys.len(), 2);
    assert!(parsed.api_keys[0].expose_secret() == first);
    assert!(parsed.api_keys[1].expose_secret() == second);
    let debug = format!("{parsed:?}");
    assert!(debug.contains("api_key_count: 2"));
    assert!(!debug.contains(&first));
    assert!(!debug.contains(&second));
}

#[test]
fn missing_or_invalid_api_key_errors_never_contain_the_argument() {
    let missing = parse_server_args(["--api-key"]);
    assert!(matches!(missing, Err(ServerArgsError::MissingApiKeyValue)));
    assert!(matches!(
        parse_server_args(["--api-key", "--host", "127.0.0.1"]),
        Err(ServerArgsError::MissingApiKeyValue)
    ));
    assert!(matches!(
        parse_server_args(["--api-key", "-h"]),
        Err(ServerArgsError::MissingApiKeyValue)
    ));

    let invalid_values = [
        "sk-secret-value-that-must-not-be-echoed".to_string(),
        format!("pk-{}", "A".repeat(64)),
        format!("sk-{}", "A".repeat(63)),
        format!("sk-{}", "A".repeat(65)),
        format!("sk-{}-", "A".repeat(63)),
    ];
    for invalid_value in invalid_values {
        let invalid = parse_server_args(["--api-key", invalid_value.as_str()])
            .expect_err("reject invalid API key format");
        assert_eq!(invalid, ServerArgsError::InvalidApiKey);
        assert!(!invalid.to_string().contains(&invalid_value));
        assert!(!format!("{invalid:?}").contains(&invalid_value));
    }
}

#[test]
fn host_accepts_ip_addresses_and_rejects_missing_or_invalid_values() {
    let ipv4 = parse_server_args(["--host", "0.0.0.0"]).expect("parse IPv4 host");
    assert_eq!(ipv4.host, IpAddr::V4(Ipv4Addr::UNSPECIFIED));

    let ipv6 = parse_server_args(["--host", "::1"]).expect("parse IPv6 host");
    assert_eq!(ipv6.host, IpAddr::V6(Ipv6Addr::LOCALHOST));

    assert!(matches!(
        parse_server_args(["--host"]),
        Err(ServerArgsError::MissingHostValue)
    ));
    assert!(matches!(
        parse_server_args(["--host", "-h"]),
        Err(ServerArgsError::MissingHostValue)
    ));
    assert!(matches!(
        parse_server_args(["--host", "localhost"]),
        Err(ServerArgsError::InvalidHost)
    ));
}

#[test]
fn standalone_process_rejects_invalid_key_before_bind_without_echoing_it() {
    let reserved = std::net::TcpListener::bind("127.0.0.1:0").expect("reserve test port");
    let port = reserved.local_addr().expect("reserved address").port();
    let invalid_value = "sk-process-secret-that-must-not-be-echoed";

    let mut command = Command::new(env!("CARGO_BIN_EXE_proxy-server"));
    command
        .arg("--api-key")
        .arg(invalid_value)
        .env("PORT", port.to_string());
    let output = output_with_timeout(command, Duration::from_secs(5));

    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(invalid_value));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!stderr.contains(invalid_value));
    assert!(stderr.contains("--api-key must use the required sk- format"));
    drop(reserved);
}
