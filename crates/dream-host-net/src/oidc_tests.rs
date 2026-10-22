use super::*;
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use bytes::Bytes;
use http_body_util::{BodyExt, Full};
use hyper::{Response, server::conn::http1, service::service_fn};
use hyper_util::rt::TokioIo;
use rand::SeedableRng;
use rsa::{RsaPrivateKey, pkcs8::EncodePrivateKey, traits::PublicKeyParts};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

#[tokio::test]
async fn verifies_discovery_pkce_and_id_tokens() {
    let key = RsaPrivateKey::new(&mut rand::rngs::StdRng::seed_from_u64(42), 2048).unwrap();
    let pem = key.to_pkcs8_pem(Default::default()).unwrap().to_string();
    let jwk = json!({"keys":[{"kty":"RSA","kid":"test","use":"sig","alg":"RS256","n":URL_SAFE_NO_PAD.encode(key.n().to_bytes_be()),"e":URL_SAFE_NO_PAD.encode(key.e().to_bytes_be())}]});
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let issuer = format!("http://{}", listener.local_addr().unwrap());
    let expected = Arc::new(Mutex::new((
        String::new(),
        String::new(),
        "client".to_owned(),
        false,
    )));
    let server_issuer = issuer.clone();
    let shared = expected.clone();
    let server = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let issuer = server_issuer.clone();
            let key = pem.clone();
            let jwk = jwk.clone();
            let shared = shared.clone();
            tokio::spawn(async move {
                let service = service_fn(move |request: hyper::Request<hyper::body::Incoming>| {
                    let issuer = issuer.clone();
                    let key = key.clone();
                    let jwk = jwk.clone();
                    let shared = shared.clone();
                    async move {
                        let path = request.uri().path().to_owned();
                        let payload = if path == "/.well-known/openid-configuration" {
                            json!({"issuer":issuer,"authorization_endpoint":format!("{issuer}/authorize"),"token_endpoint":format!("{issuer}/token"),"jwks_uri":format!("{issuer}/jwks"),"response_types_supported":["code"],"subject_types_supported":["public"],"id_token_signing_alg_values_supported":["RS256"]})
                        } else if path == "/jwks" {
                            jwk
                        } else {
                            let body = request.into_body().collect().await.unwrap().to_bytes();
                            let form: std::collections::BTreeMap<String, String> =
                                url::form_urlencoded::parse(&body).into_owned().collect();
                            let (nonce, challenge, audience, expired) =
                                shared.lock().unwrap().clone();
                            let verifier = form.get("code_verifier").cloned().unwrap_or_default();
                            if URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
                                != challenge
                            {
                                return Ok::<_, std::convert::Infallible>(
                                    Response::builder()
                                        .status(400)
                                        .header("Content-Type", "application/json")
                                        .body(Full::new(Bytes::from(
                                            "{\"error\":\"invalid_grant\"}",
                                        )))
                                        .unwrap(),
                                );
                            }
                            let now = jsonwebtoken::get_current_timestamp();
                            let mut header =
                                jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
                            header.kid = Some("test".into());
                            let token = jsonwebtoken::encode(&header, &json!({"iss":issuer,"sub":"external-user","aud":audience,"iat":now-60,"exp":if expired {now-1} else {now+600},"nonce":nonce,"email":"user@example.com","email_verified":true}), &jsonwebtoken::EncodingKey::from_rsa_pem(key.as_bytes()).unwrap()).unwrap();
                            json!({"access_token":"external-access","token_type":"Bearer","id_token":token})
                        };
                        Ok::<_, std::convert::Infallible>(
                            Response::builder()
                                .header("Content-Type", "application/json")
                                .body(Full::new(Bytes::from(payload.to_string())))
                                .unwrap(),
                        )
                    }
                });
                let _ = http1::Builder::new()
                    .serve_connection(TokioIo::new(stream), service)
                    .await;
            });
        }
    });
    let request = |completion| OidcRequest {
        issuer: &issuer,
        client_id: "client",
        client_secret: "",
        redirect_uri: "http://127.0.0.1/callback",
        completion,
    };
    let begin: serde_json::Value =
        serde_json::from_slice(&execute(&request(None)).await.unwrap().bytes).unwrap();
    let url = url::Url::parse(begin["url"].as_str().unwrap()).unwrap();
    let params: std::collections::BTreeMap<_, _> = url.query_pairs().into_owned().collect();
    assert_eq!(params["code_challenge_method"], "S256");
    assert_eq!(params["state"], begin["state"]);
    *expected.lock().unwrap() = (
        begin["nonce"].as_str().unwrap().into(),
        params["code_challenge"].clone(),
        "client".into(),
        false,
    );
    let nonce = begin["nonce"].as_str().unwrap();
    let verifier = begin["verifier"].as_str().unwrap();
    let complete = |nonce, verifier| {
        request(Some(Completion {
            code: "code",
            nonce,
            verifier,
        }))
    };
    let result: serde_json::Value =
        serde_json::from_slice(&execute(&complete(nonce, verifier)).await.unwrap().bytes).unwrap();
    assert_eq!(result["subject"], "external-user");
    assert_eq!(result["email_verified"], true);
    assert!(execute(&complete("wrong-nonce", verifier)).await.is_err());
    assert!(execute(&complete(nonce, "wrong-verifier")).await.is_err());
    expected.lock().unwrap().2 = "wrong-audience".into();
    assert!(execute(&complete(nonce, verifier)).await.is_err());
    expected.lock().unwrap().2 = "client".into();
    expected.lock().unwrap().3 = true;
    assert!(execute(&complete(nonce, verifier)).await.is_err());
    server.abort();
}
