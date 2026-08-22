use super::PkceCodes;

const AUTHORIZE_ENDPOINT: &str = "https://auth.openai.com/oauth/authorize";
const CLIENT_ID: &str = "app_EMoamEEZ73f0CkXaXp7hrann";
const SCOPE: &str = "openid profile email offline_access";

fn qp(value: &str) -> String {
    let mut encoded = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '%' => encoded.push_str("%25"),
            '&' => encoded.push_str("%26"),
            '=' => encoded.push_str("%3D"),
            '+' => encoded.push_str("%2B"),
            '#' => encoded.push_str("%23"),
            '?' => encoded.push_str("%3F"),
            ' ' => encoded.push_str("%20"),
            _ => encoded.push(ch),
        }
    }
    encoded
}

pub(crate) fn build_authorize_url(redirect_uri: &str, pkce: &PkceCodes, state: &str) -> String {
    let redirect_uri = qp(redirect_uri);
    let scope = qp(SCOPE);
    let code_challenge = qp(&pkce.challenge);
    let state = qp(state);
    format!(
        "{AUTHORIZE_ENDPOINT}\
?response_type=code\
&client_id={CLIENT_ID}\
&redirect_uri={redirect_uri}\
&scope={scope}\
&code_challenge={code_challenge}\
&code_challenge_method=S256\
&id_token_add_organizations=true\
&codex_cli_simplified_flow=true\
&state={state}\
&originator=quantix"
    )
}

#[cfg(test)]
mod tests {
    use super::super::PkceCodes;

    fn pkce() -> PkceCodes {
        PkceCodes {
            verifier: "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_string(),
            challenge: "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM".to_string(),
        }
    }

    #[test]
    fn builds_full_authorize_url_with_encoded_params() {
        let url = super::build_authorize_url(
            "http://localhost:1455/auth/callback",
            &pkce(),
            "a b%c&d=e+f#g?h",
        );
        assert_eq!(
            url,
            "https://auth.openai.com/oauth/authorize\
?response_type=code\
&client_id=app_EMoamEEZ73f0CkXaXp7hrann\
&redirect_uri=http://localhost:1455/auth/callback\
&scope=openid%20profile%20email%20offline_access\
&code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM\
&code_challenge_method=S256\
&id_token_add_organizations=true\
&codex_cli_simplified_flow=true\
&state=a%20b%25c%26d%3De%2Bf%23g%3Fh\
&originator=quantix"
        );
    }
}
