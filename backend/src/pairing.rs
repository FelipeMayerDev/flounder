//! Pareamento de instalações novas: o código fixo (`PAIRING_CODE_N` no servidor) é trocado
//! uma única vez pelo token do membro, que o app guarda localmente. Assim o cliente novo
//! baixa, digita o código e conecta — sem arquivo de configuração nenhum do lado dele.

use axum::{
    Json,
    extract::{Extension, State},
    http::StatusCode,
};
use serde::Deserialize;
use serde_json::json;
use std::{sync::Arc, time::{Duration, Instant}};
use subtle::ConstantTimeEq;

use crate::{ApiError, ApiResult, Backend, User};

/// Normaliza o código digitado: sem espaços nem traços, caixa alta.
pub fn normalize(code: &str) -> String {
    code.chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .collect::<String>()
        .to_ascii_uppercase()
}

/// Castigo global das tentativas erradas (a URL do servidor é pública): `ATTEMPTS`
/// falhas dentro da janela bloqueiam novas tentativas por um tempo igual.
#[derive(Default)]
pub struct Throttle {
    fails: u32,
    window: Option<Instant>,
    blocked_until: Option<Instant>,
}

const ATTEMPTS: u32 = 20;
const WINDOW: Duration = Duration::from_secs(10 * 60);

const MUITAS_TENTATIVAS: &str = "Muitas tentativas de código. Tente novamente mais tarde.";

impl Throttle {
    /// `Err(until)` = passou do limite agora e ficou bloqueado até `until`.
    fn register_failure(&mut self, now: Instant) -> Result<(), Instant> {
        if let Some(start) = self.window.filter(|start| now - *start < WINDOW) {
            self.window = Some(start);
        } else {
            self.window = Some(now);
            self.fails = 0;
        }
        self.fails += 1;
        if self.fails >= ATTEMPTS {
            self.blocked_until = Some(now + WINDOW);
            self.fails = 0;
            return Err(self.blocked_until.unwrap());
        }
        Ok(())
    }

    fn blocked(&self, now: Instant) -> bool {
        self.blocked_until.is_some_and(|until| now < until)
    }

    fn cleared(&mut self) {
        *self = Throttle::default();
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PairRequest {
    code: String,
}

/// Troca o código fixo pelo token do membro; rota pública, montada fora da camada de autorização.
pub async fn pair(State(state): State<Arc<Backend>>, Json(input): Json<PairRequest>) -> ApiResult {
    let now = Instant::now();
    if state.pair_throttle.lock().await.blocked(now) {
        return Err(ApiError(StatusCode::TOO_MANY_REQUESTS, MUITAS_TENTATIVAS));
    }
    let code = normalize(&input.code);
    let index = state.config.pairing_codes.iter().position(|expected| {
        expected.as_deref().is_some_and(|expected| {
            bool::from(code.as_bytes().ct_eq(normalize(expected).as_bytes()))
        })
    });
    let Some(index) = index else {
        let blocked = state
            .pair_throttle
            .lock()
            .await
            .register_failure(now)
            .is_err();
        return Err(ApiError(
            if blocked { StatusCode::TOO_MANY_REQUESTS } else { StatusCode::UNAUTHORIZED },
            if blocked { MUITAS_TENTATIVAS } else { "Código de acesso inválido." },
        ));
    };
    state.pair_throttle.lock().await.cleared();
    Ok(Json(json!({
        "client": crate::CLIENTS[index],
        "token": state.config.tokens[index],
    })))
}

/// Os códigos configurados, para quem já é do casal mandar a quem vai usar o app novo.
pub async fn codes(
    State(state): State<Arc<Backend>>,
    Extension(_user): Extension<User>,
) -> ApiResult {
    Ok(Json(json!({
        "client-1": state.config.pairing_codes[0],
        "client-2": state.config.pairing_codes[1],
    })))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_strips_separators_and_uppercases() {
        assert_eq!(normalize("maria-2026 flounder"), "MARIA2026FLOUNDER");
        assert_eq!(normalize("  F7K2-m9qp  "), "F7K2M9QP");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn throttle_blocks_after_the_limit_and_within_it() {
        let start = Instant::now();
        let mut throttle = Throttle::default();
        for _ in 0..ATTEMPTS - 1 {
            assert!(throttle.register_failure(start).is_ok());
        }
        assert!(throttle.register_failure(start).is_err(), "a falha que atinge o limite já bloqueia");
        assert!(throttle.blocked(start));
        // A janela passando não destrava: o bloqueio tem prazo próprio.
        assert!(throttle.blocked(start + WINDOW - Duration::from_secs(1)));
        assert!(!throttle.blocked(start + WINDOW + Duration::from_secs(1)));
        // Falhas espalhadas pela janela: contam juntas até virar 20.
        let mut throttle = Throttle::default();
        for i in 0..ATTEMPTS - 1 {
            throttle.register_failure(start + WINDOW * i as u32).ok();
        }
        // A 19ª falha é antiga demais: abre janela nova, zera a contagem.
        assert!(throttle.register_failure(start + WINDOW * (ATTEMPTS as u32 - 1)).is_ok());
        assert!(!throttle.blocked(start + WINDOW * (ATTEMPTS as u32 - 1)));
    }
}
