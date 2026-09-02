use std::{fs, future::Future, os::unix::fs::PermissionsExt, path::Path, pin::Pin, time::Duration};

use async_trait::async_trait;
use keyring::Entry;
use yup_oauth2::{
    authenticator_delegate::InstalledFlowDelegate,
    storage::{TokenInfo, TokenStorage, TokenStorageError},
    InstalledFlowAuthenticator, InstalledFlowReturnMethod,
};

use crate::{
    config::Paths,
    error::{AppError, Result},
};

pub const ANALYTICS_READONLY_SCOPE: &str = "https://www.googleapis.com/auth/analytics.readonly";
const KEYRING_SERVICE: &str = "ga4-omarchy";
const KEYRING_USER: &str = "google-oauth";

#[derive(Clone)]
pub struct AuthService {
    paths: Paths,
}

impl AuthService {
    pub fn new(paths: Paths) -> Self {
        Self { paths }
    }

    pub async fn install_client_secret(&self, source: &Path) -> Result<()> {
        let secret = yup_oauth2::read_application_secret(source)
            .await
            .map_err(|error| {
                AppError::InvalidInput(format!("invalid OAuth client JSON: {error}"))
            })?;
        if secret.client_id.trim().is_empty() || secret.client_secret.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "OAuth client JSON is missing a client ID or secret".into(),
            ));
        }
        fs::create_dir_all(&self.paths.config_dir)
            .map_err(|error| AppError::Config(error.to_string()))?;
        fs::copy(source, &self.paths.oauth_client_file)
            .map_err(|error| AppError::Config(error.to_string()))?;
        fs::set_permissions(
            &self.paths.oauth_client_file,
            fs::Permissions::from_mode(0o600),
        )
        .map_err(|error| AppError::Config(error.to_string()))?;
        Ok(())
    }

    pub async fn login(&self, source: &Path) -> Result<()> {
        self.install_client_secret(source).await?;
        self.logout().ok();
        self.access_token_with_prompt(true).await.map(|_| ())
    }

    pub async fn access_token(&self) -> Result<String> {
        self.access_token_with_prompt(false).await
    }

    async fn access_token_with_prompt(&self, force_account_selection: bool) -> Result<String> {
        if !self.paths.oauth_client_file.exists() {
            return Err(AppError::Unauthenticated);
        }
        let secret = yup_oauth2::read_application_secret(&self.paths.oauth_client_file)
            .await
            .map_err(|error| AppError::Config(format!("cannot read OAuth client: {error}")))?;
        let authenticator =
            InstalledFlowAuthenticator::builder(secret, InstalledFlowReturnMethod::HTTPRedirect)
                .with_storage(Box::new(KeyringTokenStorage))
                .flow_delegate(Box::new(BrowserDelegate))
                .force_account_selection(force_account_selection)
                .with_timeout(Duration::from_secs(120))
                .build()
                .await
                .map_err(|error| AppError::Other(format!("cannot initialize OAuth: {error}")))?;
        let token = authenticator
            .token(&[ANALYTICS_READONLY_SCOPE])
            .await
            .map_err(|error| authentication_error(error.to_string()))?;
        token
            .token()
            .map(str::to_owned)
            .ok_or(AppError::Unauthenticated)
    }

    pub fn status(&self) -> Result<bool> {
        match keyring_entry()?.get_password() {
            Ok(value) => Ok(!value.is_empty()),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(AppError::Keyring(error.to_string())),
        }
    }

    pub fn logout(&self) -> Result<()> {
        match keyring_entry()?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AppError::Keyring(error.to_string())),
        }
    }
}

fn authentication_error(detail: String) -> AppError {
    if detail.to_ascii_lowercase().contains("denied") {
        AppError::Other("Google authorization was denied".into())
    } else {
        AppError::Other(format!("Google authorization failed: {detail}"))
    }
}

fn keyring_entry() -> Result<Entry> {
    Entry::new(KEYRING_SERVICE, KEYRING_USER).map_err(|error| AppError::Keyring(error.to_string()))
}

struct KeyringTokenStorage;

#[async_trait]
impl TokenStorage for KeyringTokenStorage {
    async fn set(
        &self,
        _scopes: &[&str],
        token: TokenInfo,
    ) -> std::result::Result<(), TokenStorageError> {
        let encoded = serde_json::to_string(&token)
            .map_err(|error| TokenStorageError::Other(error.to_string().into()))?;
        keyring_entry()
            .and_then(|entry| {
                entry
                    .set_password(&encoded)
                    .map_err(|error| AppError::Keyring(error.to_string()))
            })
            .map_err(|error| TokenStorageError::Other(error.to_string().into()))
    }

    async fn get(&self, _scopes: &[&str]) -> Option<TokenInfo> {
        let encoded = keyring_entry().ok()?.get_password().ok()?;
        serde_json::from_str(&encoded).ok()
    }
}

struct BrowserDelegate;

impl InstalledFlowDelegate for BrowserDelegate {
    fn present_user_url<'a>(
        &'a self,
        url: &'a str,
        need_code: bool,
    ) -> Pin<Box<dyn Future<Output = std::result::Result<String, String>> + Send + 'a>> {
        Box::pin(async move {
            if let Err(error) = open::that(url) {
                eprintln!("Could not open a browser ({error}). Open this URL manually:\n{url}");
            } else {
                println!("Your browser has been opened for Google authorization.");
            }
            if need_code {
                Err("the configured OAuth client does not support a loopback redirect".into())
            } else {
                Ok(String::new())
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::tempdir;

    #[tokio::test]
    async fn invalid_client_json_is_rejected() {
        let root = tempdir().unwrap();
        let paths = Paths::from_dirs(root.path().join("config"), root.path().join("cache"));
        let source = root.path().join("invalid.json");
        fs::write(&source, "not json").unwrap();
        let error = AuthService::new(paths)
            .install_client_secret(&source)
            .await
            .unwrap_err();
        assert!(matches!(error, AppError::InvalidInput(_)));
    }

    #[tokio::test]
    async fn client_json_is_copied_with_private_permissions() {
        let root = tempdir().unwrap();
        let paths = Paths::from_dirs(root.path().join("config"), root.path().join("cache"));
        let source = root.path().join("client.json");
        fs::write(&source, r#"{"installed":{"client_id":"example.apps.googleusercontent.com","client_secret":"public-desktop-secret","auth_uri":"https://accounts.google.com/o/oauth2/auth","token_uri":"https://oauth2.googleapis.com/token","redirect_uris":["http://localhost"]}}"#).unwrap();
        AuthService::new(paths.clone())
            .install_client_secret(&source)
            .await
            .unwrap();
        assert_eq!(
            fs::metadata(paths.oauth_client_file)
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o600
        );
    }
}
