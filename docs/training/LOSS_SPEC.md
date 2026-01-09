# Training Loss Specification

## Scope
- Defines the authoritative loss used for training and validation.
- Specifies masking and normalization rules.
- Documents when metrics are comparable and when warnings are emitted.

## Authoritative Loss Definition
1) Hidden states
   - Source: base model `forward_with_hidden_states` on `input_tokens`.
   - Layer: `hidden_state_layer` if set, otherwise the last transformer layer.
   - Position: last token hidden state (current trainer behavior).
2) LoRA injection
   - `h' = h + (h @ A^T @ B^T) * (alpha / rank)`
3) Logits
   - `logits = h' @ lm_head.weight^T`
4) Loss
   - Cross-entropy over target token IDs.
   - Computed with MLX (same implementation as GPU backward pass).

## Masking Rules
- `ignore_index = 0`.
- Any target token equal to `0` is ignored by the loss.
- This assumes token ID `0` is reserved for padding and does not appear as a
  valid target token. If it does, those positions are excluded and a warning is
  emitted.

## Normalization Rules
- Loss is the mean over valid (non-ignored) tokens within an example.
- Epoch loss is the mean of per-example losses.
- Perplexity uses `exp(cross_entropy_loss)`.

## Metric Comparability
- Training and validation both use the same loss definition, mask, and
  normalization.
- Warnings are emitted when:
  - Target tokens include `ignore_index` (ignored positions).
  - All tokens are ignored (loss not comparable).

## Deprecated CPU Loss
- The legacy CPU MSE loss is deprecated and disabled in production.
- Only test builds may use it for reference or coverage.
