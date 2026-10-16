use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier, password_hash::SaltString};
use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde_json::{Value, json};

pub(crate) fn execute(request: &str) -> String {
    match serde_json::from_str(request)
        .map_err(|_| "invalid crypto request")
        .and_then(|v| operation(&v))
    {
        Ok(value) => json!({"ok": value}).to_string(),
        Err(error) => json!({"error": error}).to_string(),
    }
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a str, &'static str> {
    value[key]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or("missing crypto configuration")
}
fn operation(value: &Value) -> Result<Value, &'static str> {
    match field(value, "action")? {
        "hash_password" => {
            let password = field(value, "password")?;
            if password.len() > 1024 {
                return Err("password exceeds 1024 bytes");
            }
            let mut salt = [0; 16];
            getrandom::getrandom(&mut salt).map_err(|_| "random source failed")?;
            let salt = SaltString::encode_b64(&salt).map_err(|_| "invalid salt")?;
            Argon2::default()
                .hash_password(password.as_bytes(), &salt)
                .map(|h| json!(h.to_string()))
                .map_err(|_| "password hashing failed")
        }
        "verify_password" => {
            let password = field(value, "password")?;
            if password.len() > 1024 {
                return Ok(json!(false));
            }
            let hash =
                PasswordHash::new(field(value, "hash")?).map_err(|_| "invalid password hash")?;
            // Bound work even when a corrupt persistence adapter returns an untrusted PHC string.
            let memory = hash
                .params
                .get_decimal("m")
                .ok_or("invalid Argon2 memory cost")?;
            let time = hash
                .params
                .get_decimal("t")
                .ok_or("invalid Argon2 time cost")?;
            let lanes = hash
                .params
                .get_decimal("p")
                .ok_or("invalid Argon2 parallelism")?;
            if hash.algorithm.as_str() != "argon2id" || memory > 262_144 || time > 10 || lanes > 16
            {
                return Err("unsupported password hash parameters");
            }
            Ok(json!(
                Argon2::default()
                    .verify_password(password.as_bytes(), &hash)
                    .is_ok()
            ))
        }
        "sign" | "verify" => {
            let algorithm = match field(value, "algorithm")? {
                "RS256" => Algorithm::RS256,
                "HS256" => Algorithm::HS256,
                _ => return Err("unsupported JWT algorithm"),
            };
            let issuer = field(value, "issuer")?;
            let audience = field(value, "audience")?;
            let key = field(value, "key")?;
            if algorithm == Algorithm::HS256 && key.len() < 32 {
                return Err("HS256 requires at least 32 key bytes");
            }
            if field(value, "action")? == "sign" {
                let mut claims = value["claims"].clone();
                if !claims.is_object()
                    || claims["sub"].as_str().is_none_or(str::is_empty)
                    || claims["exp"].as_u64().is_none()
                {
                    return Err("invalid JWT claims");
                }
                claims["iss"] = json!(issuer);
                claims["aud"] = json!(audience);
                let key = if algorithm == Algorithm::HS256 {
                    EncodingKey::from_secret(key.as_bytes())
                } else {
                    EncodingKey::from_rsa_pem(key.as_bytes()).map_err(|_| "invalid signing key")?
                };
                jsonwebtoken::encode(&Header::new(algorithm), &claims, &key)
                    .map(|token| json!(token))
                    .map_err(|_| "JWT signing failed")
            } else {
                let token = field(value, "token")?;
                if token.len() > 16_384 {
                    return Err("JWT too large");
                }
                let key = if algorithm == Algorithm::HS256 {
                    DecodingKey::from_secret(key.as_bytes())
                } else {
                    DecodingKey::from_rsa_pem(key.as_bytes())
                        .map_err(|_| "invalid verification key")?
                };
                let mut validation = Validation::new(algorithm);
                validation.set_issuer(&[issuer]);
                validation.set_audience(&[audience]);
                validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
                validation.leeway = 0;
                validation.validate_nbf = true;
                jsonwebtoken::decode::<Value>(token, &key, &validation)
                    .map(|token| token.claims)
                    .map_err(|_| "invalid access token")
            }
        }
        _ => Err("unknown crypto action"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn passwords_and_invalid_hashes() {
        let hashed =
            operation(&json!({"action":"hash_password","password":"correct horse"})).unwrap();
        assert!(hashed.as_str().unwrap().starts_with("$argon2id$"));
        assert_eq!(
            operation(
                &json!({"action":"verify_password","password":"correct horse","hash":hashed})
            )
            .unwrap(),
            true
        );
        assert_eq!(
            operation(&json!({"action":"verify_password","password":"wrong","hash":hashed}))
                .unwrap(),
            false
        );
        assert!(
            operation(&json!({"action":"verify_password","password":"wrong","hash":"bad"}))
                .is_err()
        );
    }
    #[test]
    fn jwt_verification_enforces_policy() {
        let key = "0123456789abcdef0123456789abcdef";
        let mut request = json!({"action":"sign","algorithm":"HS256","issuer":"test","audience":"api","key":key,"claims":{"sub":"1","exp":jsonwebtoken::get_current_timestamp()+60}});
        let token = operation(&request).unwrap();
        request["token"] = token;
        request["action"] = json!("verify");
        assert_eq!(operation(&request).unwrap()["sub"], "1");
        request["audience"] = json!("wrong");
        assert!(operation(&request).is_err());
        request["audience"] = json!("api");
        request["key"] = json!("abcdef0123456789abcdef0123456789");
        assert!(operation(&request).is_err());
        request["algorithm"] = json!("none");
        assert!(operation(&request).is_err());
    }
}
