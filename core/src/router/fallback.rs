//! 故障转移与熔断
//!
//! 提供 Provider fallback 链与基础 circuit breaker 能力。

use std::time::{Duration, Instant};

#[derive(Debug, Clone, Default)]
struct ProviderState {
    consecutive_failures: u32,
    open_until: Option<Instant>,
}

pub struct FallbackRouter {
    chain: Vec<String>,
    failure_threshold: u32,
    cooldown: Duration,
    states: std::collections::HashMap<String, ProviderState>,
}

impl FallbackRouter {
    pub fn with_config(chain: Vec<String>, failure_threshold: u32, cooldown: Duration) -> Self {
        Self {
            chain,
            failure_threshold,
            cooldown,
            states: std::collections::HashMap::new(),
        }
    }

    pub fn select_next(&mut self, preferred: &str, now: Instant) -> Option<String> {
        self.available_candidates(preferred, now).into_iter().next()
    }

    pub fn available_candidates(&self, preferred: &str, now: Instant) -> Vec<String> {
        if self.chain.is_empty() {
            return Vec::new();
        }

        let mut ordered = Vec::with_capacity(self.chain.len());
        if self.chain.iter().any(|p| p == preferred) {
            ordered.push(preferred.to_string());
        }

        for provider in &self.chain {
            if provider != preferred {
                ordered.push(provider.clone());
            }
        }

        ordered
            .into_iter()
            .filter(|provider| !self.is_circuit_open(provider, now))
            .collect()
    }

    pub fn record_success(&mut self, provider: &str) {
        let state = self.states.entry(provider.to_string()).or_default();
        state.consecutive_failures = 0;
        state.open_until = None;
    }

    pub fn record_failure(&mut self, provider: &str, now: Instant) {
        let state = self.states.entry(provider.to_string()).or_default();
        state.consecutive_failures = state.consecutive_failures.saturating_add(1);

        if state.consecutive_failures >= self.failure_threshold {
            state.open_until = Some(now + self.cooldown);
        }
    }

    pub fn is_circuit_open(&self, provider: &str, now: Instant) -> bool {
        let Some(state) = self.states.get(provider) else {
            return false;
        };

        matches!(state.open_until, Some(until) if now < until)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_circuit_after_threshold_failures() {
        let mut router = FallbackRouter::with_config(
            vec!["local".to_string(), "openai".to_string()],
            3,
            Duration::from_secs(30),
        );
        let now = Instant::now();

        router.record_failure("local", now);
        router.record_failure("local", now);
        assert!(!router.is_circuit_open("local", now));

        router.record_failure("local", now);
        assert!(router.is_circuit_open("local", now));
    }

    #[test]
    fn falls_back_to_next_when_primary_open() {
        let mut router = FallbackRouter::with_config(
            vec![
                "local".to_string(),
                "openai".to_string(),
                "anthropic".to_string(),
            ],
            2,
            Duration::from_secs(30),
        );
        let now = Instant::now();

        router.record_failure("local", now);
        router.record_failure("local", now);

        let selected = router.select_next("local", now);
        assert_eq!(selected.as_deref(), Some("openai"));
    }

    #[test]
    fn half_open_after_cooldown_and_recover_on_success() {
        let mut router = FallbackRouter::with_config(
            vec!["local".to_string(), "openai".to_string()],
            2,
            Duration::from_secs(30),
        );

        let now = Instant::now();
        router.record_failure("local", now);
        router.record_failure("local", now);
        assert!(router.is_circuit_open("local", now));

        let later = now + Duration::from_secs(31);
        assert!(!router.is_circuit_open("local", later));

        let selected = router.select_next("local", later);
        assert_eq!(selected.as_deref(), Some("local"));

        router.record_success("local");
        assert!(!router.is_circuit_open("local", later));
    }
}
