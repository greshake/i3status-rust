use base64::Engine as _;
use oauth2::basic::{
    BasicErrorResponse, BasicRevocationErrorResponse, BasicTokenIntrospectionResponse,
    BasicTokenResponse,
};
use oauth2::{
    AuthUrl, AuthorizationCode, Client, ClientId, ClientSecret, CsrfToken, EndpointNotSet,
    EndpointSet, PkceCodeChallenge, PkceCodeVerifier, RedirectUrl, RefreshToken, Scope,
    StandardRevocableToken, TokenResponse as _, TokenUrl,
};
use reqwest;
use reqwest::Url;
use reqwest::header::{AUTHORIZATION, HeaderMap, HeaderValue};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tokio::fs::File;
use tokio::io::{AsyncBufReadExt as _, AsyncReadExt as _, AsyncWriteExt as _, BufReader};
use tokio::net::TcpListener;

use super::CalendarError;
use crate::{APP_USER_AGENT, REQWEST_TIMEOUT};

static REQWEST_CLIENT: LazyLock<reqwest::Client> = LazyLock::new(|| {
    reqwest::Client::builder()
        .user_agent(APP_USER_AGENT)
        .timeout(REQWEST_TIMEOUT)
        // Following redirects opens the client up to SSRF vulnerabilities.
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .unwrap()
});

type BasicClient<
    HasAuthUrl = EndpointSet,
    HasDeviceAuthUrl = EndpointNotSet,
    HasIntrospectionUrl = EndpointNotSet,
    HasRevocationUrl = EndpointNotSet,
    HasTokenUrl = EndpointSet,
> = Client<
    BasicErrorResponse,
    BasicTokenResponse,
    BasicTokenIntrospectionResponse,
    StandardRevocableToken,
    BasicRevocationErrorResponse,
    HasAuthUrl,
    HasDeviceAuthUrl,
    HasIntrospectionUrl,
    HasRevocationUrl,
    HasTokenUrl,
>;

pub enum Auth {
    Unauthenticated,
    Basic(Basic),
    OAuth2(Box<OAuth2>),
}

impl Auth {
    pub fn oauth2(flow: OAuth2Flow, token_store: TokenStore, scopes: Vec<Scope>) -> Self {
        Self::OAuth2(Box::new(OAuth2 {
            flow,
            token_store,
            scopes,
        }))
    }
    pub fn basic(username: String, password: String) -> Self {
        Self::Basic(Basic { username, password })
    }
    pub async fn headers(&mut self) -> HeaderMap {
        match self {
            Auth::Unauthenticated => HeaderMap::new(),
            Auth::Basic(auth) => auth.headers().await,
            Auth::OAuth2(auth) => auth.headers().await,
        }
    }

    pub async fn handle_error(&mut self, error: reqwest::Error) -> Result<(), CalendarError> {
        match self {
            Auth::Unauthenticated | Auth::Basic(_) => Err(CalendarError::Http(error)),
            Auth::OAuth2(auth) => auth.handle_error(error).await,
        }
    }

    pub async fn authorize(&mut self) -> Result<Authorize, CalendarError> {
        match self {
            Auth::Unauthenticated | Auth::Basic(_) => Ok(Authorize::Completed),
            Auth::OAuth2(auth) => Ok(Authorize::AskUser(auth.authorize().await?)),
        }
    }
    pub async fn ask_user(&mut self, authorize_url: AuthorizeUrl) -> Result<(), CalendarError> {
        match self {
            Auth::Unauthenticated | Auth::Basic(_) => Ok(()),
            Auth::OAuth2(auth) => auth.ask_user(authorize_url).await,
        }
    }
}

pub struct Basic {
    username: String,
    password: String,
}

impl Basic {
    pub async fn headers(&mut self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        let header =
            base64::prelude::BASE64_STANDARD.encode(format!("{}:{}", self.username, self.password));
        let mut header_value = HeaderValue::from_str(format!("Basic {header}").as_str())
            .expect("A valid basic header");
        header_value.set_sensitive(true);
        headers.insert(AUTHORIZATION, header_value);
        headers
    }
}

pub struct OAuth2 {
    flow: OAuth2Flow,
    token_store: TokenStore,
    scopes: Vec<Scope>,
}

impl OAuth2 {
    pub async fn headers(&mut self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        if let Some(token) = self.token_store.get().await {
            let mut auth_value =
                HeaderValue::from_str(format!("Bearer {}", token.access_token().secret()).as_str())
                    .expect("A valid access token");
            auth_value.set_sensitive(true);
            headers.insert(AUTHORIZATION, auth_value);
        }
        headers
    }

    async fn handle_error(&mut self, error: reqwest::Error) -> Result<(), CalendarError> {
        if let Some(status) = error.status() {
            if status == 401 {
                match self
                    .token_store
                    .get()
                    .await
                    .and_then(|t| t.refresh_token().cloned())
                {
                    Some(refresh_token) => {
                        let mut token = self.flow.refresh_token_exchange(&refresh_token).await?;
                        if token.refresh_token().is_none() {
                            token.set_refresh_token(Some(refresh_token));
                        }
                        self.token_store.store(token).await?;
                        return Ok(());
                    }
                    None => return Err(CalendarError::AuthRequired),
                }
            }
            if status == 403 {
                return Err(CalendarError::AuthRequired);
            }
        }
        Err(CalendarError::Http(error))
    }

    async fn authorize(&mut self) -> Result<AuthorizeUrl, CalendarError> {
        Ok(self.flow.authorize_url(self.scopes.clone()))
    }

    async fn ask_user(&mut self, authorize_url: AuthorizeUrl) -> Result<(), CalendarError> {
        let token = self.flow.redirect(authorize_url).await?;
        self.token_store.store(token).await?;
        Ok(())
    }
}
pub struct OAuth2Flow {
    client: BasicClient,
    redirect_port: u16,
}

impl OAuth2Flow {
    pub fn new(
        client_id: ClientId,
        client_secret: ClientSecret,
        auth_url: AuthUrl,
        token_url: TokenUrl,
        redirect_port: u16,
    ) -> Self {
        Self {
            client: BasicClient::new(client_id)
                .set_client_secret(client_secret)
                .set_auth_uri(auth_url)
                .set_token_uri(token_url)
                .set_redirect_uri(
                    RedirectUrl::new(format!("http://localhost:{redirect_port}").to_string())
                        .expect("A valid redirect URL"),
                ),
            redirect_port,
        }
    }

    pub fn authorize_url(&self, scopes: Vec<Scope>) -> AuthorizeUrl {
        let (pkce_code_challenge, pkce_code_verifier) = PkceCodeChallenge::new_random_sha256();
        let (authorize_url, csrf_token) = self
            .client
            .authorize_url(CsrfToken::new_random)
            .add_scopes(scopes)
            .set_pkce_challenge(pkce_code_challenge.clone())
            .url();
        AuthorizeUrl {
            pkce_code_verifier,
            url: authorize_url,
            csrf_token,
        }
    }

    pub async fn refresh_token_exchange(
        &self,
        token: &RefreshToken,
    ) -> Result<BasicTokenResponse, CalendarError> {
        self.client
            .exchange_refresh_token(token)
            .request_async(&*REQWEST_CLIENT)
            .await
            .map_err(|e| CalendarError::RequestToken(e.to_string()))
    }

    pub async fn redirect(
        &self,
        authorize_url: AuthorizeUrl,
    ) -> Result<BasicTokenResponse, CalendarError> {
        let client = self.client.clone();
        let redirect_port = self.redirect_port;
        let listener = TcpListener::bind(format!("127.0.0.1:{redirect_port}")).await?;
        let (mut stream, _) = listener.accept().await?;
        let mut request_line = String::new();
        let mut reader = BufReader::new(&mut stream);
        reader.read_line(&mut request_line).await?;

        let redirect_url = request_line
            .split_whitespace()
            .nth(1)
            .ok_or(CalendarError::RequestToken("Invalid redirect url".into()))?;
        let url = Url::parse(&("http://localhost".to_string() + redirect_url))
            .map_err(|e| CalendarError::RequestToken(e.to_string()))?;

        let (_, code_value) =
            url.query_pairs()
                .find(|(key, _)| key == "code")
                .ok_or(CalendarError::RequestToken(
                    "code query param is missing".into(),
                ))?;
        let code = AuthorizationCode::new(code_value.into_owned());
        let (_, state_value) = url.query_pairs().find(|(key, _)| key == "state").ok_or(
            CalendarError::RequestToken("state query param is missing".into()),
        )?;
        let state = CsrfToken::new(state_value.into_owned());
        if state.secret() != authorize_url.csrf_token.secret() {
            return Err(CalendarError::RequestToken(
                "Received state and csrf token are different".to_string(),
            ));
        }

        let message = "Now your i3status-rust calendar is authorized";
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-length: {}\r\n\r\n{}",
            message.len(),
            message
        );
        stream.write_all(response.as_bytes()).await?;

        client
            .exchange_code(code)
            .set_pkce_verifier(authorize_url.pkce_code_verifier)
            .request_async(&*REQWEST_CLIENT)
            .await
            .map_err(|e| CalendarError::RequestToken(e.to_string()))
    }
}

#[derive(Debug)]
pub enum Authorize {
    Completed,
    AskUser(AuthorizeUrl),
}

#[derive(Debug)]
pub struct AuthorizeUrl {
    pkce_code_verifier: PkceCodeVerifier,
    pub url: Url,
    csrf_token: CsrfToken,
}

#[derive(Debug)]
pub struct TokenStore {
    path: PathBuf,
    token: Option<BasicTokenResponse>,
}

impl TokenStore {
    pub fn new(path: &Path) -> Self {
        Self {
            path: path.into(),
            token: None,
        }
    }

    pub async fn store(&mut self, token: BasicTokenResponse) -> Result<(), TokenStoreError> {
        let mut file = File::create(&self.path).await?;
        let value = serde_json::to_string(&token)?;
        file.write_all(value.as_bytes()).await?;
        self.token = Some(token);
        Ok(())
    }

    pub async fn get(&mut self) -> Option<BasicTokenResponse> {
        if self.token.is_none()
            && let Ok(mut file) = File::open(&self.path).await
        {
            let mut content = vec![];
            file.read_to_end(&mut content).await.ok()?;
            self.token = serde_json::from_slice(&content).ok();
        }
        self.token.clone()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum TokenStoreError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}
