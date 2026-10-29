//! The runs' tokens and what each allows: a run's requests are known by its token, and refused once the card's or the
//! run's token budget, or the day's, is spent.

use std::sync::Arc;

use axum::http::{HeaderMap, StatusCode, header};
use axum::response::Response;

use super::{Protocol, Router, error};
use crate::store::Usage;

/// What a run's token allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Grant {
    pub run_id: i64,
    pub card_id: Option<i64>,
    /// The card's token budget, or the run's when it has no card.
    pub budget: Option<i64>,
}

impl Router {
    /// Refuses model requests once `tokens` have been used today by all runs.
    pub fn with_day_budget(self, tokens: Option<i64>) -> Self {
        self.set_day_budget(tokens);
        self
    }

    /// Changes the day's budget; the next request is checked against it.
    pub fn set_day_budget(&self, tokens: Option<i64>) {
        *self.day_budget.write().unwrap_or_else(|p| p.into_inner()) = tokens;
    }

    fn day_budget(&self) -> Option<i64> {
        *self.day_budget.read().unwrap_or_else(|p| p.into_inner())
    }

    /// Issues a new token for a run; requests carrying it are counted against the run.
    pub fn issue_token(&self, grant: Grant) -> String {
        let mut bytes = [0u8; 24];
        getrandom::fill(&mut bytes).expect("the system has randomness");
        let token = format!("moebius-run-{}", hex::encode(bytes));
        self.grants
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .insert(token.clone(), grant);
        token
    }

    /// Withdraws a run's token when the run ends.
    pub fn revoke_token(&self, token: &str) {
        self.grants
            .write()
            .unwrap_or_else(|p| p.into_inner())
            .remove(token);
    }

    /// How many tokens are valid, for tests that check a token is withdrawn.
    #[cfg(test)]
    pub fn token_count(&self) -> usize {
        self.grants.read().unwrap_or_else(|p| p.into_inner()).len()
    }

    /// What the token in the request's headers allows, or none for an unknown or missing token.
    pub fn grant(&self, headers: &HeaderMap) -> Option<Grant> {
        let token = headers
            .get(header::AUTHORIZATION)
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .or_else(|| headers.get("x-api-key").and_then(|v| v.to_str().ok()))?;
        self.grants
            .read()
            .unwrap_or_else(|p| p.into_inner())
            .get(token)
            .cloned()
    }

    /// Why the grant's request must be refused: the day's budget or the grant's budget is spent.
    fn overspent(&self, grant: &Grant) -> crate::store::Result<Option<String>> {
        if let Some(day) = self.day_budget() {
            let today = self.store.tokens_today()?;
            if today >= day {
                return Ok(Some(format!(
                    "the daily token budget of {day} is spent ({today} used today)"
                )));
            }
        }
        let Some(budget) = grant.budget else {
            return Ok(None);
        };
        let spent = match grant.card_id {
            Some(card) => self.store.tokens_for_card(card)?,
            None => self
                .store
                .usage_for_run(grant.run_id)?
                .iter()
                .map(Usage::total)
                .sum(),
        };
        Ok((spent >= budget)
            .then(|| format!("the token budget of {budget} is spent ({spent} used)")))
    }

    /// A refusal when a budget is spent or the usage cannot be read.
    pub(super) async fn check_budget(
        self: Arc<Self>,
        protocol: Protocol,
        grant: &Grant,
    ) -> Option<Response> {
        if self.day_budget().is_none() && grant.budget.is_none() {
            return None;
        }
        let g = grant.clone();
        match tokio::task::spawn_blocking(move || self.overspent(&g)).await {
            Ok(Ok(Some(message))) => Some(error(
                protocol,
                StatusCode::TOO_MANY_REQUESTS,
                "budget_exceeded",
                &message,
            )),
            Ok(Ok(None)) => None,
            _ => Some(error(
                protocol,
                StatusCode::INTERNAL_SERVER_ERROR,
                "api_error",
                "cannot read usage",
            )),
        }
    }
}
