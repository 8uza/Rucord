use oauth2::{
    basic::BasicClient, AuthUrl, ClientId, CsrfToken,
    PkceCodeChallenge, RedirectUrl, Scope, TokenResponse, TokenUrl,
};
use tiny_http::{Server, Response};

const AUTH_URL: &str = "https://discord.com/api/oauth2/authorize";
const TOKEN_URL: &str = "https://discord.com/api/oauth2/token";
const REDIRECT: &str = "http://localhost:8080/callback";

pub struct OAuthResult {
    pub access_token: String,
}

pub fn run_oauth_flow(client_id: &str) -> Result<OAuthResult, String> {
    let client = BasicClient::new(
        ClientId::new(client_id.to_string()),
        None,
        AuthUrl::new(AUTH_URL.to_string()).map_err(|e| e.to_string())?,
        Some(TokenUrl::new(TOKEN_URL.to_string()).map_err(|e| e.to_string())?),
    )
    .set_redirect_uri(RedirectUrl::new(REDIRECT.to_string()).map_err(|e| e.to_string())?);

    let (pkce_challenge, pkce_verifier) = PkceCodeChallenge::new_random_sha256();
    let (auth_url, _csrf) = client
        .authorize_url(CsrfToken::new_random)
        .add_scope(Scope::new("identify".to_string()))
        .add_scope(Scope::new("guilds".to_string()))
        .set_pkce_challenge(pkce_challenge)
        .url();

    let _ = open::that(auth_url.to_string());

    let server = Server::http("0.0.0.0:8080").map_err(|e| e.to_string())?;
    let mut code: Option<String> = None;
    for request in server.incoming_requests() {
        let url = request.url().to_string();
        if let Some(q) = url.split('?').nth(1) {
            for pair in q.split('&') {
                let mut kv = pair.split('=');
                if kv.next() == Some("code") {
                    code = kv.next().map(|s| s.to_string());
                }
            }
        }
        let _ = request.respond(Response::from_string("You can close this window."));
        if code.is_some() {
            break;
        }
    }
    let code = code.ok_or_else(|| "no code received".to_string())?;

    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
    let token = rt.block_on(async {
        let resp = client
            .exchange_code(oauth2::AuthorizationCode::new(code))
            .set_pkce_verifier(pkce_verifier)
            .request_async(oauth2::reqwest::async_http_client)
            .await
            .map_err(|e| e.to_string())?;
        Ok::<_, String>(resp.access_token().secret().to_string())
    })?;
    Ok(OAuthResult { access_token: token })
}
