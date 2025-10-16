use adapteros_core::{AosError, Result};

#[derive(Clone)]
pub(crate) struct SimpleTokenizer {
    vocab_size: usize,
    eos_token_id: u32,
}

impl SimpleTokenizer {
    pub(crate) fn new(vocab_size: usize) -> Self {
        Self {
            vocab_size,
            eos_token_id: vocab_size.saturating_sub(1) as u32,
        }
    }

    pub(crate) fn encode(&self, text: &str) -> Result<Vec<u32>> {
        let mut tokens = Vec::with_capacity(text.len());
        for ch in text.chars() {
            let code = ch as u32;
            if code as usize >= self.vocab_size {
                return Err(AosError::Validation(format!(
                    "Character '{}' (code {}) exceeds vocabulary size {}",
                    ch, code, self.vocab_size
                )));
            }
            tokens.push(code);
        }
        Ok(tokens)
    }

    pub(crate) fn decode(&self, ids: &[u32]) -> Result<String> {
        let mut output = String::with_capacity(ids.len());
        for &id in ids {
            if id == self.eos_token_id {
                break;
            }
            let ch = std::char::from_u32(id)
                .ok_or_else(|| AosError::Validation(format!("Invalid UTF-32 codepoint: {}", id)))?;
            output.push(ch);
        }
        Ok(output)
    }

    pub(crate) fn eos_token_id(&self) -> u32 {
        self.eos_token_id
    }
}

pub(crate) fn run_generation_loop<F>(
    tokenizer: &SimpleTokenizer,
    mut tokens: Vec<u32>,
    max_tokens: usize,
    vocab_size: usize,
    max_context: usize,
    mut step_fn: F,
) -> Result<Vec<u32>>
where
    F: FnMut(&[u32]) -> Result<(Vec<f32>, usize)>,
{
    if tokens.is_empty() {
        return Err(AosError::Validation(
            "Generation loop requires at least one token".to_string(),
        ));
    }

    if tokens.len() >= max_context {
        return Err(AosError::Validation(
            "Initial tokens already exhaust context window".to_string(),
        ));
    }

    let mut generated = Vec::new();
    for step in 0..max_tokens {
        let (logits, hidden_count) = step_fn(&tokens)?;
        tracing::trace!(
            "generation_logits",
            step,
            logits_len = logits.len(),
            hidden_count
        );
        if logits.is_empty() {
            return Err(AosError::Mlx("Received empty logits from MLX".to_string()));
        }

        let fallback = tokens.last().copied().unwrap_or(0);
        let next_id = select_next_token(&logits, fallback, step, vocab_size)?;
        tokens.push(next_id);

        if next_id == tokenizer.eos_token_id() {
            tracing::info!("Generation reached EOS at step {}", step);
            break;
        }

        generated.push(next_id);

        if tokens.len() >= max_context {
            tracing::warn!("Context window reached at step {}", step);
            break;
        }
    }

    Ok(generated)
}

fn select_next_token(logits: &[f32], fallback: u32, step: usize, vocab_size: usize) -> Result<u32> {
    if vocab_size == 0 {
        return Err(AosError::Validation(
            "Vocab size must be greater than zero".to_string(),
        ));
    }

    if logits.len() == vocab_size {
        let mut best_idx = 0usize;
        let mut best_val = f32::NEG_INFINITY;
        for (idx, &value) in logits.iter().enumerate() {
            if value.is_nan() {
                continue;
            }
            if value > best_val {
                best_val = value;
                best_idx = idx;
            }
        }
        return Ok(best_idx as u32);
    }

    Ok(((fallback as usize + step + 1) % vocab_size) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenizer_round_trip() {
        let tokenizer = SimpleTokenizer::new(512);
        let text = "Hello";
        let ids = tokenizer.encode(text).unwrap();
        assert_eq!(tokenizer.decode(&ids).unwrap(), text);
    }

    #[test]
    fn tokenizer_rejects_out_of_vocab() {
        let tokenizer = SimpleTokenizer::new(128);
        assert!(matches!(
            tokenizer.encode("Ā"),
            Err(AosError::Validation(_))
        ));
    }

    #[test]
    fn select_uses_max_logit() {
        let logits = vec![0.1, 0.2, 0.9, 0.3];
        assert_eq!(select_next_token(&logits, 0, 0, 4).unwrap(), 2);
    }

    #[test]
    fn generation_fallback_path() {
        let tokenizer = SimpleTokenizer::new(16);
        let output = run_generation_loop(&tokenizer, vec![1u32], 3, 16, 32, |_tokens| {
            Ok((vec![0.0; 2], 0))
        })
        .unwrap();
        assert!(!output.is_empty());
    }

    #[test]
    fn generation_initial_context_violation() {
        let tokenizer = SimpleTokenizer::new(16);
        let err = run_generation_loop(&tokenizer, vec![1u32, 2u32], 3, 16, 2, |_tokens| {
            Ok((vec![0.0; 16], 0))
        })
        .unwrap_err();
        assert!(matches!(err, AosError::Validation(_)));
    }
}
