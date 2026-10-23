use super::*;

fn command(id: i32, action: &str, url: &str, cookie: &str) -> serde_json::Value {
    serde_json::from_str(&control(
        id,
        &serde_json::json!({"action": action, "url": url, "cookie": cookie}).to_string(),
    ))
    .unwrap()
}

#[test]
fn cookie_scope_replacement_expiry_and_invalid_input() {
    let jar = create();
    assert!(jar > 0);
    let origin = "https://example.test/private/page";
    for cookie in [
        "host=1; Path=/",
        "domain=1; Domain=example.test; Path=/",
        "private=1; Path=/private",
        "secure=1; Secure; Path=/",
        "replaced=old; Path=/",
        "replaced=new; Path=/",
        "expired=1; Max-Age=0; Path=/",
        "past=1; Expires=Thu, 01 Jan 1970 00:00:00 GMT; Path=/",
        "wrong=1; Domain=other.test; Path=/",
        "invalid",
    ] {
        assert_eq!(command(jar, "store", origin, cookie)["value"], "");
    }
    let header = command(jar, "header", origin, "")["value"]
        .as_str()
        .unwrap()
        .to_owned();
    for expected in [
        "host=1",
        "domain=1",
        "private=1",
        "secure=1",
        "replaced=new",
    ] {
        assert!(header.split("; ").any(|pair| pair == expected), "{header}");
    }
    for rejected in ["old", "expired", "past", "wrong", "invalid"] {
        assert!(!header.contains(rejected), "{header}");
    }
    let public = command(jar, "header", "http://example.test/public", "");
    let public = public["value"].as_str().unwrap();
    assert!(!public.contains("private"));
    assert!(!public.contains("secure"));
    assert_eq!(
        command(jar, "header", "https://child.example.test/", "")["value"],
        "domain=1"
    );
    assert_eq!(
        command(jar, "header", "https://other.test/", "")["value"],
        ""
    );
    for url in [
        "file:///tmp/a",
        "https://user:password@example.test",
        "bad url",
    ] {
        assert_eq!(command(jar, "header", url, "")["code"], "EINVAL");
    }
    assert_eq!(
        command(jar, "store", origin, "bad=1\r\nInjected: 1")["code"],
        "EINVAL"
    );
    assert_eq!(command(jar, "clear", "", "")["value"], "");
    assert_eq!(command(jar, "header", origin, "")["value"], "");
    close(jar);
    assert_eq!(command(jar, "header", origin, "")["code"], "ECLOSED");
}

#[test]
fn public_suffixes_prefixes_and_path_creation_order() {
    let jar = create();
    let attacker = "https://attacker.co.uk/";
    command(
        jar,
        "store",
        attacker,
        "session=injected; Domain=co.uk; Path=/",
    );
    assert_eq!(
        command(jar, "header", "https://victim.co.uk/", "")["value"],
        ""
    );
    let handle = get(jar).unwrap();
    let cookie = HeaderValue::from_static("session=injected; Domain=github.io; Path=/");
    handle.set_cookies(
        &mut [&cookie].into_iter(),
        &url::Url::parse("https://attacker.github.io/").unwrap(),
    );
    assert_eq!(
        command(jar, "header", "https://victim.github.io/", "")["value"],
        ""
    );
    let origin = "https://example.test/private/";
    for cookie in [
        "session=wide; Path=/",
        "first=1; Path=/private",
        "session=narrow; Path=/private",
        "first=2; Path=/private",
    ] {
        command(jar, "store", origin, cookie);
    }
    assert_eq!(
        command(jar, "header", origin, "")["value"],
        "first=2; session=narrow; session=wide"
    );
    command(jar, "clear", "", "");
    for cookie in [
        "__Host-session=bad; Domain=example.test; Secure; Path=/",
        "__Host-session=bad; Secure",
        "__Secure-session=bad",
    ] {
        command(jar, "store", origin, cookie);
    }
    assert_eq!(command(jar, "header", origin, "")["value"], "");
    command(jar, "store", origin, "__Host-session=good; Secure; Path=/");
    assert_eq!(
        command(jar, "header", origin, "")["value"],
        "__Host-session=good"
    );
    close(jar);
}
