//! Sampling DTO → domain validation (SPEC §30).
//!
//! Normalizes `SamplingDto` into `SamplingConfig` with range-checked values.
//! The `thinking` field accepts both numeric and boolean YAML forms; this
//! submodule collapses them into a single canonical `Option<f64>`.

#![allow(clippy::result_large_err)]

use crate::config::domain::SamplingConfig;
use crate::config::dto::{SamplingDto, ThinkingDto};
use crate::error::AilError;

use super::cfg_err;

/// Validate and normalize a `SamplingDto` into a `SamplingConfig`.
///
/// - `scope` is a human-readable label used in error messages (e.g.
///   `"pipeline defaults"`, `"provider"`, `"step 'brainstorm'"`).
/// - Returns `Ok(None)` when the DTO is `None` or every field is absent.
/// - Range checks (SPEC §30.6.1):
///   - `temperature` ∈ [0.0, 2.0]
///   - `top_p` ∈ [0.0, 1.0]
///   - `top_k` ≥ 1
///   - `max_tokens` ≥ 1
///   - `stop_sequences`: non-empty list of non-empty strings
///   - `thinking` ∈ [0.0, 1.0] after normalizing booleans
pub(in crate::config) fn validate_sampling(
    dto: Option<SamplingDto>,
    scope: &str,
) -> Result<Option<SamplingConfig>, AilError> {
    let Some(dto) = dto else {
        return Ok(None);
    };

    if let Some(t) = dto.temperature {
        if !(0.0..=2.0).contains(&t) || !t.is_finite() {
            return Err(cfg_err!(
                "{scope}: sampling.temperature must be a finite number in [0.0, 2.0]; got {t}"
            ));
        }
    }
    if let Some(p) = dto.top_p {
        if !(0.0..=1.0).contains(&p) || !p.is_finite() {
            return Err(cfg_err!(
                "{scope}: sampling.top_p must be a finite number in [0.0, 1.0]; got {p}"
            ));
        }
    }
    if let Some(k) = dto.top_k {
        if k < 1 {
            return Err(cfg_err!("{scope}: sampling.top_k must be >= 1; got {k}"));
        }
    }
    if let Some(m) = dto.max_tokens {
        if m < 1 {
            return Err(cfg_err!(
                "{scope}: sampling.max_tokens must be >= 1; got {m}"
            ));
        }
    }
    if let Some(ref stops) = dto.stop_sequences {
        if stops.is_empty() {
            return Err(cfg_err!(
                "{scope}: sampling.stop_sequences must be a non-empty list when set"
            ));
        }
        for (i, s) in stops.iter().enumerate() {
            if s.is_empty() {
                return Err(cfg_err!(
                    "{scope}: sampling.stop_sequences[{i}] must be a non-empty string"
                ));
            }
        }
    }

    let thinking = match dto.thinking {
        None => None,
        Some(raw) => {
            let n = match raw {
                ThinkingDto::Number(n) => n,
                ThinkingDto::Bool(true) => 1.0,
                ThinkingDto::Bool(false) => 0.0,
            };
            if !(0.0..=1.0).contains(&n) || !n.is_finite() {
                return Err(cfg_err!(
                    "{scope}: sampling.thinking must be a finite number in [0.0, 1.0] \
                     or a boolean; got {n}"
                ));
            }
            Some(n)
        }
    };

    let cfg = SamplingConfig {
        temperature: dto.temperature,
        top_p: dto.top_p,
        top_k: dto.top_k,
        max_tokens: dto.max_tokens,
        stop_sequences: dto.stop_sequences,
        thinking,
    };

    if cfg.is_empty() {
        Ok(None)
    } else {
        Ok(Some(cfg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_returns_none() {
        assert!(validate_sampling(None, "test").unwrap().is_none());
    }

    #[test]
    fn all_absent_returns_none() {
        let dto = SamplingDto::default();
        assert!(validate_sampling(Some(dto), "test").unwrap().is_none());
    }

    #[test]
    fn thinking_bool_true_normalizes_to_one() {
        let dto = SamplingDto {
            thinking: Some(ThinkingDto::Bool(true)),
            ..Default::default()
        };
        let cfg = validate_sampling(Some(dto), "test").unwrap().unwrap();
        assert_eq!(cfg.thinking, Some(1.0));
    }

    #[test]
    fn thinking_bool_false_normalizes_to_zero() {
        let dto = SamplingDto {
            thinking: Some(ThinkingDto::Bool(false)),
            ..Default::default()
        };
        let cfg = validate_sampling(Some(dto), "test").unwrap().unwrap();
        assert_eq!(cfg.thinking, Some(0.0));
    }

    #[test]
    fn thinking_float_passes_through() {
        let dto = SamplingDto {
            thinking: Some(ThinkingDto::Number(0.7)),
            ..Default::default()
        };
        let cfg = validate_sampling(Some(dto), "test").unwrap().unwrap();
        assert_eq!(cfg.thinking, Some(0.7));
    }

    #[test]
    fn temperature_out_of_range_errors() {
        let dto = SamplingDto {
            temperature: Some(3.0),
            ..Default::default()
        };
        assert!(validate_sampling(Some(dto), "test").is_err());
    }

    #[test]
    fn top_p_out_of_range_errors() {
        let dto = SamplingDto {
            top_p: Some(1.5),
            ..Default::default()
        };
        assert!(validate_sampling(Some(dto), "test").is_err());
    }

    #[test]
    fn top_k_zero_errors() {
        let dto = SamplingDto {
            top_k: Some(0),
            ..Default::default()
        };
        assert!(validate_sampling(Some(dto), "test").is_err());
    }

    #[test]
    fn max_tokens_zero_errors() {
        let dto = SamplingDto {
            max_tokens: Some(0),
            ..Default::default()
        };
        assert!(validate_sampling(Some(dto), "test").is_err());
    }

    #[test]
    fn stop_sequences_empty_list_errors() {
        let dto = SamplingDto {
            stop_sequences: Some(vec![]),
            ..Default::default()
        };
        assert!(validate_sampling(Some(dto), "test").is_err());
    }

    #[test]
    fn stop_sequences_empty_string_errors() {
        let dto = SamplingDto {
            stop_sequences: Some(vec!["ok".to_string(), "".to_string()]),
            ..Default::default()
        };
        assert!(validate_sampling(Some(dto), "test").is_err());
    }

    #[test]
    fn thinking_out_of_range_errors() {
        let dto = SamplingDto {
            thinking: Some(ThinkingDto::Number(1.5)),
            ..Default::default()
        };
        assert!(validate_sampling(Some(dto), "test").is_err());
    }

    #[test]
    fn full_config_roundtrips() {
        let dto = SamplingDto {
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            max_tokens: Some(4096),
            stop_sequences: Some(vec!["Human:".to_string()]),
            thinking: Some(ThinkingDto::Number(0.5)),
        };
        let cfg = validate_sampling(Some(dto), "test").unwrap().unwrap();
        assert_eq!(cfg.temperature, Some(0.7));
        assert_eq!(cfg.top_p, Some(0.9));
        assert_eq!(cfg.top_k, Some(40));
        assert_eq!(cfg.max_tokens, Some(4096));
        assert_eq!(
            cfg.stop_sequences.as_deref(),
            Some(&["Human:".to_string()][..])
        );
        assert_eq!(cfg.thinking, Some(0.5));
    }
}
