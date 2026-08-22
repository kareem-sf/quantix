const AUTH_CLAIMS_KEY: &str = "https://api.openai.com/auth";

pub(crate) fn parse_jwt_claims(token: &str) -> Option<serde_json::Value> {
    let payload = token.split('.').nth(1)?;
    let bytes = super::crypto::base64url_decode(payload).ok()?;
    serde_json::from_slice(&bytes).ok()
}

pub(crate) struct ChatGptIdentity {
    pub account_id: String,
    pub plan_type: Option<String>,
}

pub(crate) fn extract_identity(token: &str) -> Option<ChatGptIdentity> {
    let claims = parse_jwt_claims(token)?;
    let auth = claims.get(AUTH_CLAIMS_KEY);
    let account_id = claims
        .get("chatgpt_account_id")
        .and_then(serde_json::Value::as_str)
        .or_else(|| {
            auth.and_then(|value| value.get("chatgpt_account_id"))
                .and_then(serde_json::Value::as_str)
        })
        .or_else(|| {
            claims
                .get("organizations")
                .and_then(|value| value.get(0))
                .and_then(|value| value.get("id"))
                .and_then(serde_json::Value::as_str)
        })?
        .to_string();
    let plan_type = auth
        .and_then(|value| value.get("chatgpt_plan_type"))
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);
    Some(ChatGptIdentity {
        account_id,
        plan_type,
    })
}

#[cfg(test)]
mod tests {
    use crate::chatgpt_oauth::crypto::base64url_encode;
    use serde_json::json;

    fn token_for(claims: serde_json::Value) -> String {
        let header = base64url_encode(br#"{"alg":"RS256","typ":"JWT"}"#);
        let payload = base64url_encode(claims.to_string().as_bytes());
        format!("{header}.{payload}.c2ln")
    }

    #[test]
    fn parses_payload_claims_from_token() {
        let token = token_for(json!({"sub": "user-1"}));
        let claims = super::parse_jwt_claims(&token).unwrap();
        assert_eq!(claims["sub"], "user-1");
    }

    #[test]
    fn prefers_root_account_id_and_reads_nested_plan() {
        let token = token_for(json!({
            "chatgpt_account_id": "acc-root",
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acc-nested",
                "chatgpt_plan_type": "plus"
            },
            "organizations": [{"id": "acc-org"}]
        }));
        let identity = super::extract_identity(&token).unwrap();
        assert_eq!(identity.account_id, "acc-root");
        assert_eq!(identity.plan_type.as_deref(), Some("plus"));
    }

    #[test]
    fn falls_back_to_nested_auth_claim() {
        let token = token_for(json!({
            "https://api.openai.com/auth": {
                "chatgpt_account_id": "acc-nested",
                "chatgpt_plan_type": "team"
            },
            "organizations": [{"id": "acc-org"}]
        }));
        let identity = super::extract_identity(&token).unwrap();
        assert_eq!(identity.account_id, "acc-nested");
        assert_eq!(identity.plan_type.as_deref(), Some("team"));
    }

    #[test]
    fn falls_back_to_first_organization() {
        let token = token_for(json!({
            "organizations": [
                {"id": "acc-org", "is_default": true},
                {"id": "acc-org-2"}
            ]
        }));
        let identity = super::extract_identity(&token).unwrap();
        assert_eq!(identity.account_id, "acc-org");
        assert_eq!(identity.plan_type, None);
    }

    #[test]
    fn returns_none_without_any_account_id() {
        let token = token_for(json!({"email": "engineer@example.com"}));
        assert!(super::extract_identity(&token).is_none());
    }

    #[test]
    fn returns_none_on_malformed_tokens() {
        assert!(super::parse_jwt_claims("header.!!!not-base64!!!.sig").is_none());
        assert!(super::parse_jwt_claims("no-dots").is_none());
        assert!(super::extract_identity("no-dots").is_none());
    }
}
