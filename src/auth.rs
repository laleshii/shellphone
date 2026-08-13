use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use rand::Rng;
use std::sync::Arc;
use subtle::ConstantTimeEq;
use tokio::sync::Mutex;

pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::rng().fill(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.as_bytes().ct_eq(b.as_bytes()).into()
}

pub enum AuthResult {
    NewSession { refresh_token: String },
    Reconnected,
    Failed,
}

#[derive(Clone)]
pub struct SessionGuard {
    initial_token: String,
    initial_consumed: Arc<Mutex<bool>>,
    refresh_token: Arc<Mutex<Option<String>>>,
}

impl SessionGuard {
    pub fn new(token: String) -> Self {
        Self {
            initial_token: token,
            initial_consumed: Arc::new(Mutex::new(false)),
            refresh_token: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn authenticate(
        &self,
        initial: Option<&str>,
        refresh: Option<&str>,
    ) -> AuthResult {
        if let Some(candidate) = refresh {
            let stored = self.refresh_token.lock().await;
            if let Some(ref expected) = *stored {
                if constant_time_eq(candidate, expected) {
                    return AuthResult::Reconnected;
                }
            }
        }

        if let Some(candidate) = initial {
            if !constant_time_eq(candidate, &self.initial_token) {
                return AuthResult::Failed;
            }

            let mut consumed = self.initial_consumed.lock().await;
            if *consumed {
                return AuthResult::Failed;
            }
            *consumed = true;

            let refresh = generate_token();
            *self.refresh_token.lock().await = Some(refresh.clone());
            return AuthResult::NewSession {
                refresh_token: refresh,
            };
        }

        AuthResult::Failed
    }
}
