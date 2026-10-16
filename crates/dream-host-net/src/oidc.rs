use crate::socket_operations::{Failure, Output};
use openidconnect::{
    AccessTokenHash, AuthenticationFlow, AuthorizationCode, ClientId, ClientSecret, CsrfToken,
    IssuerUrl, Nonce, OAuth2TokenResponse, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, Scope,
    core::{CoreClient, CoreProviderMetadata, CoreResponseType},
    reqwest,
};
use serde_json::{Value, json};
use std::time::Duration;

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str, Failure> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or(("EINVAL", format!("missing OIDC {key}")))
}
fn failure(message: &str) -> Failure {
    ("EAUTH", message.to_owned())
}
fn allowed_url(value: &str) -> bool {
    url::Url::parse(value).is_ok_and(|url| {
        url.username().is_empty()
            && url.password().is_none()
            && url.fragment().is_none()
            && (url.scheme() == "https"
                || (url.scheme() == "http"
                    && matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"))))
    })
}
pub(crate) async fn execute(value: &Value) -> Result<Output, Failure> {
    let issuer = text(value, "issuer")?;
    let redirect = text(value, "redirect_uri")?;
    if !allowed_url(issuer) || !allowed_url(redirect) {
        return Err(failure("OIDC requires HTTPS or loopback URLs"));
    }
    let http = reqwest::ClientBuilder::new()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|_| failure("OIDC client configuration failed"))?;
    let issuer_url =
        IssuerUrl::new(issuer.to_owned()).map_err(|_| failure("invalid OIDC issuer"))?;
    let metadata = CoreProviderMetadata::discover_async(issuer_url, &http)
        .await
        .map_err(|_| failure("OIDC discovery failed"))?;
    if !allowed_url(metadata.authorization_endpoint().url().as_str())
        || !allowed_url(metadata.jwks_uri().url().as_str())
        || metadata
            .token_endpoint()
            .is_none_or(|endpoint| !allowed_url(endpoint.url().as_str()))
    {
        return Err(failure("OIDC discovery contains an insecure endpoint"));
    }
    let client = CoreClient::from_provider_metadata(
        metadata,
        ClientId::new(text(value, "client_id")?.to_owned()),
        value["client_secret"]
            .as_str()
            .filter(|s| !s.is_empty())
            .map(|s| ClientSecret::new(s.to_owned())),
    )
    .set_redirect_uri(
        RedirectUrl::new(redirect.to_owned()).map_err(|_| failure("invalid OIDC redirect"))?,
    );
    let result = match text(value, "action")? {
        "oidc_begin" => {
            let (challenge, verifier) = PkceCodeChallenge::new_random_sha256();
            let (authorize, state, nonce) = client
                .authorize_url(
                    AuthenticationFlow::<CoreResponseType>::AuthorizationCode,
                    CsrfToken::new_random,
                    Nonce::new_random,
                )
                .set_pkce_challenge(challenge)
                .add_scope(Scope::new("profile".to_owned()))
                .add_scope(Scope::new("email".to_owned()))
                .url();
            json!({"url":authorize.to_string(),"state":state.secret(),"nonce":nonce.secret(),"verifier":verifier.secret()})
        }
        "oidc_complete" => {
            let response = client
                .exchange_code(AuthorizationCode::new(text(value, "code")?.to_owned()))
                .map_err(|_| failure("missing OIDC token endpoint"))?
                .set_pkce_verifier(PkceCodeVerifier::new(text(value, "verifier")?.to_owned()))
                .request_async(&http)
                .await
                .map_err(|_| failure("OIDC code exchange failed"))?;
            let id = response
                .extra_fields()
                .id_token()
                .ok_or_else(|| failure("missing OIDC ID token"))?;
            let verifier = client.id_token_verifier();
            let claims = id
                .claims(&verifier, &Nonce::new(text(value, "nonce")?.to_owned()))
                .map_err(|_| failure("invalid OIDC ID token"))?;
            if let Some(expected) = claims.access_token_hash() {
                let actual = AccessTokenHash::from_token(
                    response.access_token(),
                    id.signing_alg()
                        .map_err(|_| failure("invalid OIDC signing algorithm"))?,
                    id.signing_key(&verifier)
                        .map_err(|_| failure("invalid OIDC signing key"))?,
                )
                .map_err(|_| failure("invalid OIDC access token hash"))?;
                if actual != *expected {
                    return Err(failure("OIDC access token mismatch"));
                }
            }
            json!({"issuer":claims.issuer().as_str(),"subject":claims.subject().as_str(),"email":claims.email().map(|e| e.as_str()),"email_verified":claims.email_verified().unwrap_or(false)})
        }
        _ => return Err(failure("unsupported OIDC operation")),
    };
    Ok(Output::bytes(
        serde_json::to_vec(&result).map_err(|_| failure("OIDC encoding failed"))?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn requires_trusted_transport_urls() {
        assert!(allowed_url("https://accounts.example.com"));
        assert!(allowed_url("http://127.0.0.1:9000/callback"));
        assert!(!allowed_url("http://example.com"));
        assert!(!allowed_url("https://user:secret@example.com"));
        assert!(!allowed_url("https://example.com/#fragment"));
    }
}

#[cfg(test)]
#[path = "oidc_tests.rs"]
mod integration_tests;
