use dream_host_abi::{handles::HandleTable, lock};
use reqwest::{cookie::CookieStore, header::HeaderValue};
use serde::Deserialize;
use std::{
    cmp::Reverse,
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

#[derive(Default)]
pub(crate) struct CookieJar(Mutex<CookieState>);

type CookieKey = (String, String, String);

#[derive(Default)]
struct CookieState {
    store: cookie_store::CookieStore,
    creation: BTreeMap<CookieKey, u64>,
    next_creation: u64,
}

impl CookieState {
    fn key(cookie: &cookie_store::Cookie<'_>) -> CookieKey {
        (
            String::from(&cookie.domain),
            String::from(&cookie.path),
            cookie.name().to_owned(),
        )
    }

    fn store(&mut self, value: &str, url: &url::Url) {
        let Ok(cookie) = cookie_store::Cookie::parse(value, url) else {
            return;
        };
        let mut cookie = cookie.into_owned();
        if let cookie_store::CookieDomain::Suffix(domain) = &cookie.domain
            && psl::suffix_str(domain).is_some_and(|suffix| suffix == domain)
        {
            if url.host_str() != Some(domain) {
                return;
            }
            cookie.domain = cookie_store::CookieDomain::HostOnly(domain.clone());
        }
        let secure = cookie.secure() == Some(true) && url.scheme() == "https";
        if cookie.name().starts_with("__Secure-") && !secure {
            return;
        }
        if cookie.name().starts_with("__Host-")
            && (!secure
                || cookie.domain().is_some()
                || !cookie.path.is_from_path_attr()
                || &*cookie.path != "/")
        {
            return;
        }
        let key = Self::key(&cookie);
        self.creation
            .retain(|(domain, path, name), _| self.store.contains(domain, path, name));
        if self.store.insert(cookie, url).is_ok() && !self.creation.contains_key(&key) {
            self.creation.insert(key, self.next_creation);
            self.next_creation += 1;
        }
    }

    fn header(&self, url: &url::Url) -> String {
        let mut cookies = self.store.matches(url);
        cookies.sort_by_key(|cookie| {
            (
                Reverse(cookie.path.len()),
                self.creation.get(&Self::key(cookie)).copied().unwrap_or(0),
            )
        });
        cookies
            .iter()
            .map(|cookie| format!("{}={}", cookie.name(), cookie.value()))
            .collect::<Vec<_>>()
            .join("; ")
    }
}

impl CookieStore for CookieJar {
    fn set_cookies(&self, headers: &mut dyn Iterator<Item = &HeaderValue>, url: &url::Url) {
        let mut state = lock(&self.0);
        for header in headers {
            if let Ok(value) = header.to_str() {
                state.store(value, url);
            }
        }
    }

    fn cookies(&self, url: &url::Url) -> Option<HeaderValue> {
        let value = lock(&self.0).header(url);
        (!value.is_empty())
            .then(|| HeaderValue::from_str(&value).ok())
            .flatten()
    }
}

fn registry() -> &'static Mutex<HandleTable<Arc<CookieJar>>> {
    static JARS: Mutex<HandleTable<Arc<CookieJar>>> = Mutex::new(HandleTable::new());
    &JARS
}

pub(crate) fn create() -> i32 {
    lock(registry())
        .insert(Arc::new(CookieJar::default()))
        .unwrap_or(0)
}

pub(crate) fn get(id: i32) -> Option<Arc<CookieJar>> {
    lock(registry()).get(id).cloned()
}

pub(crate) fn close(id: i32) {
    lock(registry()).remove(id);
}

#[derive(Deserialize)]
struct Command {
    action: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    cookie: String,
}

pub(crate) fn control(id: i32, input: &str) -> String {
    let result = (|| {
        let command: Command =
            serde_json::from_str(input).map_err(|_| ("EINVAL", "invalid cookie command"))?;
        let jar = get(id).ok_or(("ECLOSED", "cookie jar is unavailable"))?;
        let mut jar = lock(&jar.0);
        if command.action == "clear" {
            *jar = CookieState::default();
            return Ok(String::new());
        }
        let url = url::Url::parse(&command.url).map_err(|_| ("EINVAL", "invalid cookie URL"))?;
        if !matches!(url.scheme(), "http" | "https")
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return Err((
                "EINVAL",
                "cookie URL must use http/https without credentials",
            ));
        }
        match command.action.as_str() {
            "store" => {
                HeaderValue::from_str(&command.cookie)
                    .map_err(|_| ("EINVAL", "invalid cookie header"))?;
                jar.store(&command.cookie, &url);
                Ok(String::new())
            }
            "header" => Ok(jar.header(&url)),
            _ => Err(("EINVAL", "unknown cookie command")),
        }
    })();
    match result {
        Ok(value) => serde_json::json!({"value": value}).to_string(),
        Err((code, message)) => serde_json::json!({"code": code, "message": message}).to_string(),
    }
}

#[cfg(test)]
#[path = "cookie_tests.rs"]
mod tests;
