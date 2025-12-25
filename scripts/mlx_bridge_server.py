#!/usr/bin/env python3
"""
MLX Bridge Server - Subprocess bridge for MoE model inference

This script runs as a subprocess spawned by the Rust worker, providing
inference capabilities for MoE models via Python's mlx-lm library.

Protocol:
- Input: JSON requests on stdin (one per line)
- Output: JSON responses on stdout (one per line)
- Stderr: Logging and error messages

Request format:
{
    "type": "generate",
    "prompt": "def hello():",
    "max_tokens": 50,
    "temperature": 0.7,
    "top_p": 0.9,
    "stop_sequences": [],
    "stream": false,
    "protocol_version": 2
}

Response format:
{
    "type": "generate_response",
    "text": "generated text",
    "tokens": 42,
    "finish_reason": "stop"
}

OR for streaming (protocol_version: 2):
{
    "type": "stream_token",
    "token": "hello",
    "index": 0,
    "token_id": 12345
}
{
    "type": "stream_end",
    "tokens": 42,
    "finish_reason": "stop",
    "text": "complete generated text",
    "usage": {"prompt_tokens": 10, "completion_tokens": 42, "total_tokens": 52},
    "timing": {"ttft_ms": 150.0, "total_ms": 2500.0, "tokens_per_second": 16.8}
}

Test with:
  # Start bridge with streaming
  echo '{"type":"generate", "prompt":"def hello():", "max_tokens":50, "stream":true}' | \\
    MLX_MODEL_PATH=./var/models/Qwen2.5-7B-Instruct-4bit python3 scripts/mlx_bridge_server.py

Backward compatibility:
  - Non-streaming requests (stream: false) still return generate_response
  - protocol_version is optional, defaults to 1
  - Older clients ignoring new fields (usage, timing, token_id) will still work
"""

import json
import os
import sys
import time
import traceback
from pathlib import Path
from typing import Optional, Iterator, Any

def log(msg: str):
    """Log to stderr (stdout is reserved for JSON responses)"""
    print(f"[MLX-BRIDGE] {msg}", file=sys.stderr, flush=True)

def send_response(response: dict):
    """Send JSON response to stdout"""
    print(json.dumps(response), flush=True)

def send_error(error_msg: str, error_type: str = "error"):
    """Send error response"""
    send_response({
        "type": "error",
        "error": error_msg,
        "error_type": error_type
    })

class MLXBridgeServer:
    PROTOCOL_VERSION = 3  # Current protocol version (v3 adds expert routing)

    def __init__(self, model_path: str):
        self.model_path = model_path
        self.model = None
        self.tokenizer = None
        self.stream_generate_fn = None  # Optional stream_generate function
        self.is_moe = False  # Whether model is Mixture of Experts
        self.num_experts = 0  # Number of experts if MoE
        self.experts_per_token = 0  # Active experts per token
        self.expert_routing_enabled = False  # Whether to track expert routing

    def initialize(self):
        """Initialize MLX model and tokenizer"""
        try:
            log(f"Initializing MLX model from: {self.model_path}")

            # Import mlx-lm
            try:
                from mlx_lm import load, generate
                self.load_fn = load
                self.generate_fn = generate
                log("mlx-lm imported successfully")

                # Try to import stream_generate for true streaming support
                try:
                    from mlx_lm import stream_generate
                    self.stream_generate_fn = stream_generate
                    log("mlx-lm stream_generate available")
                except ImportError:
                    log("mlx-lm stream_generate not available, will use fallback")
                    self.stream_generate_fn = None
            except ImportError as e:
                raise RuntimeError(f"Failed to import mlx-lm. Is it installed? Error: {e}")

            # Load model and tokenizer
            log("Loading model and tokenizer...")
            try:
                # Try standard load first
                self.model, self.tokenizer = self.load_fn(self.model_path)
            except Exception as e:
                # Fallback: load with slow tokenizer for problematic tokenizer.json files
                log(f"Standard load failed: {e}")
                log("Attempting load with slow tokenizer (use_fast=False)...")
                from transformers import AutoTokenizer
                from mlx_lm import utils

                # Temporarily rename the corrupt tokenizer.json if it exists
                tokenizer_json = Path(self.model_path) / "tokenizer.json"
                tokenizer_backup = Path(self.model_path) / "tokenizer.json.backup"
                renamed = False

                try:
                    if tokenizer_json.exists():
                        log(f"Temporarily renaming {tokenizer_json} to avoid corruption...")
                        tokenizer_json.rename(tokenizer_backup)
                        renamed = True

                    # Load model using mlx-lm utils
                    log("Loading model weights with mlx_lm.utils.load_model...")
                    model_result = utils.load_model(Path(self.model_path), lazy=False)
                    # load_model returns (model, config) tuple
                    if isinstance(model_result, tuple):
                        self.model, model_config = model_result
                    else:
                        self.model = model_result
                    log(f"Model loaded: {type(self.model).__name__}")

                    # Load slow tokenizer
                    log("Loading slow tokenizer...")
                    self.tokenizer = AutoTokenizer.from_pretrained(
                        self.model_path,
                        use_fast=False,
                        trust_remote_code=True
                    )
                    log("Slow tokenizer loaded successfully")

                except Exception as e2:
                    log(f"Fallback load failed: {e2}")
                    # Restore backup if needed
                    if renamed and tokenizer_backup.exists():
                        tokenizer_backup.rename(tokenizer_json)
                    raise

                finally:
                    # Restore the tokenizer.json if we renamed it
                    if renamed and tokenizer_backup.exists() and not tokenizer_json.exists():
                        tokenizer_backup.rename(tokenizer_json)

            log(f"Model loaded successfully: {type(self.model).__name__}")

            # Detect MoE configuration
            self._detect_moe_config()

            # Send ready message
            send_response({
                "type": "ready",
                "model_path": self.model_path,
                "model_type": type(self.model).__name__,
                "protocol_version": self.PROTOCOL_VERSION,
                "streaming_supported": self.stream_generate_fn is not None,
                "is_moe": self.is_moe,
                "num_experts": self.num_experts,
                "experts_per_token": self.experts_per_token
            })

        except Exception as e:
            error_msg = f"Failed to initialize model: {str(e)}\n{traceback.format_exc()}"
            log(error_msg)
            send_error(error_msg, "initialization_error")
            sys.exit(1)

    def _detect_moe_config(self):
        """Detect if model is MoE and extract configuration"""
        try:
            # Check model args for MoE indicators
            if hasattr(self.model, 'args'):
                args = self.model.args
                # Qwen3-MoE style
                if hasattr(args, 'num_experts') and args.num_experts > 0:
                    self.is_moe = True
                    self.num_experts = args.num_experts
                    self.experts_per_token = getattr(args, 'num_experts_per_tok', 8)
                    log(f"MoE detected: {self.num_experts} experts, {self.experts_per_token} per token")
                # Mixtral style
                elif hasattr(args, 'num_local_experts') and args.num_local_experts > 0:
                    self.is_moe = True
                    self.num_experts = args.num_local_experts
                    self.experts_per_token = getattr(args, 'num_experts_per_tok', 2)
                    log(f"MoE (Mixtral-style) detected: {self.num_experts} experts")

            # Fallback: check model type name
            if not self.is_moe:
                model_type = type(self.model).__name__.lower()
                if 'moe' in model_type or 'mixture' in model_type:
                    self.is_moe = True
                    log(f"MoE detected from model type: {type(self.model).__name__}")

            if self.is_moe:
                self.expert_routing_enabled = True
                log("Expert routing instrumentation enabled")

        except Exception as e:
            log(f"MoE detection failed (continuing without): {e}")

    def _get_expert_routing(self, layer_idx: int) -> Optional[list]:
        """Get expert routing for a layer during inference (if available)

        Returns list of (expert_idx, routing_weight) tuples for active experts
        """
        if not self.is_moe or not self.expert_routing_enabled:
            return None

        try:
            # Try to access the last routing decision from the model
            # This is model-architecture specific
            if hasattr(self.model, 'model') and hasattr(self.model.model, 'layers'):
                layers = self.model.model.layers
                if layer_idx < len(layers):
                    layer = layers[layer_idx]
                    # Qwen-style MoE block
                    if hasattr(layer, 'mlp') and hasattr(layer.mlp, 'gate'):
                        gate = layer.mlp.gate
                        if hasattr(gate, 'last_routing'):
                            routing = gate.last_routing
                            if routing is not None:
                                # Return top-k experts with their weights
                                import mlx.core as mx
                                if isinstance(routing, mx.array):
                                    routing = routing.tolist()
                                return routing
        except Exception:
            pass

        return None

    def _collect_all_expert_routing(self) -> Optional[list]:
        """Collect expert routing from all layers after a forward pass

        Returns: list of (layer_idx, expert_idx) tuples
        """
        if not self.is_moe or not self.expert_routing_enabled:
            return None

        routing_data = []
        try:
            if hasattr(self.model, 'model') and hasattr(self.model.model, 'layers'):
                num_layers = len(self.model.model.layers)
                for layer_idx in range(num_layers):
                    layer_routing = self._get_expert_routing(layer_idx)
                    if layer_routing:
                        for expert_idx in layer_routing:
                            if isinstance(expert_idx, (int, float)):
                                routing_data.append((layer_idx, int(expert_idx)))
                            elif isinstance(expert_idx, (list, tuple)):
                                # (expert_idx, weight) tuple
                                routing_data.append((layer_idx, int(expert_idx[0])))
        except Exception as e:
            log(f"Expert routing collection failed: {e}")

        return routing_data if routing_data else None

    def _stream_tokens_native(self, prompt: str, max_tokens: int, sampler: Any) -> Iterator[tuple[str, int]]:
        """Stream tokens using native mlx_lm.stream_generate"""
        for response in self.stream_generate_fn(
            model=self.model,
            tokenizer=self.tokenizer,
            prompt=prompt,
            max_tokens=max_tokens,
            sampler=sampler,
        ):
            # stream_generate yields GenerateResult objects with text and token
            # The text is cumulative, so we need to extract individual tokens
            yield response.text, response.token

    def _stream_tokens_fallback(self, prompt: str, max_tokens: int, sampler: Any) -> Iterator[tuple[str, int]]:
        """Fallback streaming: generate all at once then emit tokens"""
        result = self.generate_fn(
            model=self.model,
            tokenizer=self.tokenizer,
            prompt=prompt,
            max_tokens=max_tokens,
            sampler=sampler,
            verbose=False
        )

        # Tokenize the result to get individual tokens
        token_ids = self.tokenizer.encode(result)
        for i, token_id in enumerate(token_ids):
            token_text = self.tokenizer.decode([token_id])
            yield token_text, token_id

    def handle_generate(self, request: dict):
        """Handle generate request"""
        try:
            prompt = request.get("prompt", "")
            max_tokens = request.get("max_tokens", 100)
            temperature = request.get("temperature", 0.7)
            top_p = request.get("top_p", 0.9)
            stream = request.get("stream", False)
            protocol_version = request.get("protocol_version", 1)

            # Create sampler with temperature and top_p
            from mlx_lm.sample_utils import make_sampler
            sampler = make_sampler(temp=temperature, top_p=top_p)

            # Protocol v3: check if routing collection is requested
            collect_routing = request.get("collect_routing", False) and self.is_moe and protocol_version >= 3
            all_routing_data = [] if collect_routing else None

            log(f"Generating: max_tokens={max_tokens}, temp={temperature}, top_p={top_p}, stream={stream}, protocol_v={protocol_version}, routing={collect_routing}")

            start_time = time.perf_counter()
            first_token_time = None

            if stream:
                # Streaming generation
                token_count = 0
                generated_text = ""
                prompt_tokens = len(self.tokenizer.encode(prompt))

                # Choose streaming method
                if self.stream_generate_fn is not None:
                    # Use native stream_generate
                    try:
                        prev_text = ""
                        for cumulative_text, token_id in self._stream_tokens_native(prompt, max_tokens, sampler):
                            # Extract the new token (stream_generate gives cumulative text)
                            new_token = cumulative_text[len(prev_text):]
                            prev_text = cumulative_text

                            if first_token_time is None:
                                first_token_time = time.perf_counter()

                            # Collect routing data if requested
                            token_routing = None
                            if collect_routing:
                                token_routing = self._collect_all_expert_routing()
                                if token_routing:
                                    all_routing_data.append(token_routing)

                            response = {
                                "type": "stream_token",
                                "token": new_token,
                                "index": token_count,
                                "token_id": token_id
                            }
                            if token_routing:
                                response["routing"] = token_routing
                            send_response(response)
                            generated_text = cumulative_text
                            token_count += 1
                    except Exception as e:
                        log(f"Native streaming failed: {e}, falling back")
                        # Fall through to fallback
                        if token_count == 0:
                            for token_text, token_id in self._stream_tokens_fallback(prompt, max_tokens, sampler):
                                if first_token_time is None:
                                    first_token_time = time.perf_counter()

                                send_response({
                                    "type": "stream_token",
                                    "token": token_text,
                                    "index": token_count,
                                    "token_id": token_id
                                })
                                generated_text += token_text
                                token_count += 1
                else:
                    # Use fallback streaming
                    for token_text, token_id in self._stream_tokens_fallback(prompt, max_tokens, sampler):
                        if first_token_time is None:
                            first_token_time = time.perf_counter()

                        send_response({
                            "type": "stream_token",
                            "token": token_text,
                            "index": token_count,
                            "token_id": token_id
                        })
                        generated_text += token_text
                        token_count += 1

                end_time = time.perf_counter()
                total_time_ms = (end_time - start_time) * 1000
                ttft_ms = (first_token_time - start_time) * 1000 if first_token_time else total_time_ms

                # Send stream end with usage stats
                stream_end_response = {
                    "type": "stream_end",
                    "tokens": token_count,
                    "finish_reason": "stop",
                    "text": generated_text,
                    "usage": {
                        "prompt_tokens": prompt_tokens,
                        "completion_tokens": token_count,
                        "total_tokens": prompt_tokens + token_count
                    },
                    "timing": {
                        "ttft_ms": round(ttft_ms, 2),
                        "total_ms": round(total_time_ms, 2),
                        "tokens_per_second": round(token_count / (total_time_ms / 1000), 2) if total_time_ms > 0 else 0
                    }
                }
                # Include MoE metadata if this is a MoE model
                if self.is_moe:
                    stream_end_response["moe_info"] = {
                        "is_moe": True,
                        "num_experts": self.num_experts,
                        "experts_per_token": self.experts_per_token
                    }
                # Include aggregated routing data if collected
                if all_routing_data:
                    stream_end_response["expert_routing"] = all_routing_data
                send_response(stream_end_response)
            else:
                # Non-streaming generation
                result = self.generate_fn(
                    model=self.model,
                    tokenizer=self.tokenizer,
                    prompt=prompt,
                    max_tokens=max_tokens,
                    sampler=sampler,
                    verbose=False
                )

                end_time = time.perf_counter()
                total_time_ms = (end_time - start_time) * 1000

                # Get accurate token counts
                prompt_tokens = len(self.tokenizer.encode(prompt))
                completion_tokens = len(self.tokenizer.encode(result))

                send_response({
                    "type": "generate_response",
                    "text": result,
                    "tokens": completion_tokens,
                    "finish_reason": "stop",
                    "usage": {
                        "prompt_tokens": prompt_tokens,
                        "completion_tokens": completion_tokens,
                        "total_tokens": prompt_tokens + completion_tokens
                    },
                    "timing": {
                        "total_ms": round(total_time_ms, 2),
                        "tokens_per_second": round(completion_tokens / (total_time_ms / 1000), 2) if total_time_ms > 0 else 0
                    }
                })

        except Exception as e:
            error_msg = f"Generation failed: {str(e)}\n{traceback.format_exc()}"
            log(error_msg)
            send_error(error_msg, "generation_error")

    def handle_health_check(self):
        """Handle health check request"""
        send_response({
            "type": "health_response",
            "status": "healthy",
            "model_loaded": self.model is not None
        })

    def handle_diagnose(self):
        """Handle diagnose request - dump model internals for debugging"""
        try:
            import mlx.core as mx

            diagnosis = {
                "type": "diagnose_response",
                "model_type": type(self.model).__name__,
                "model_path": self.model_path,
            }

            # Get model config if available
            if hasattr(self.model, 'args'):
                args = self.model.args
                diagnosis["config"] = {
                    "hidden_size": getattr(args, 'hidden_size', None),
                    "num_hidden_layers": getattr(args, 'num_hidden_layers', None),
                    "num_attention_heads": getattr(args, 'num_attention_heads', None),
                    "num_key_value_heads": getattr(args, 'num_key_value_heads', None),
                    "vocab_size": getattr(args, 'vocab_size', None),
                    "intermediate_size": getattr(args, 'intermediate_size', None),
                }

            # Recursively collect all weights from mlx_lm model structure
            weight_keys = []
            def collect_all_weights(obj, prefix=''):
                """Recursively collect all mx.array weights from model structure"""
                if isinstance(obj, mx.array):
                    weight_keys.append({
                        "key": prefix,
                        "shape": list(obj.shape),
                        "dtype": str(obj.dtype)
                    })
                elif isinstance(obj, dict):
                    for k, v in obj.items():
                        new_prefix = f'{prefix}.{k}' if prefix else k
                        collect_all_weights(v, new_prefix)
                elif isinstance(obj, (list, tuple)):
                    for i, v in enumerate(obj):
                        new_prefix = f'{prefix}.{i}' if prefix else str(i)
                        collect_all_weights(v, new_prefix)
                elif hasattr(obj, 'parameters') and callable(obj.parameters):
                    # It's a module - get its parameters
                    try:
                        params = obj.parameters()
                        collect_all_weights(params, prefix)
                    except Exception:
                        pass
                    # Also check for .layers attribute
                    if hasattr(obj, 'layers'):
                        layers = obj.layers
                        if isinstance(layers, (list, tuple)):
                            for i, layer in enumerate(layers):
                                layer_prefix = f'{prefix}.layers.{i}' if prefix else f'layers.{i}'
                                collect_all_weights(layer, layer_prefix)

            try:
                collect_all_weights(self.model)
                log(f"Collected {len(weight_keys)} weights recursively")
            except Exception as e:
                log(f"Weight collection failed: {e}")

            diagnosis["weight_count"] = len(weight_keys)
            diagnosis["weight_keys"] = weight_keys[:50]  # First 50 to avoid huge responses
            diagnosis["weight_keys_truncated"] = len(weight_keys) > 50

            # Count layers
            layer_count = 0
            for wk in weight_keys:
                key = wk["key"]
                if ".layers." in key:
                    try:
                        # Extract layer number from patterns like "model.layers.27.self_attn"
                        parts = key.split(".layers.")
                        if len(parts) > 1:
                            layer_num = int(parts[1].split(".")[0])
                            layer_count = max(layer_count, layer_num + 1)
                    except (ValueError, IndexError):
                        pass

            diagnosis["detected_layers"] = layer_count

            # Check for common weight patterns
            key_set = {wk["key"] for wk in weight_keys}
            diagnosis["has_embed_tokens"] = any("embed_tokens" in k for k in key_set)
            diagnosis["has_lm_head"] = any("lm_head" in k for k in key_set)
            diagnosis["has_model_norm"] = any("model.norm" in k or "final_norm" in k for k in key_set)

            send_response(diagnosis)

        except Exception as e:
            error_msg = f"Diagnose failed: {str(e)}\n{traceback.format_exc()}"
            log(error_msg)
            send_error(error_msg, "diagnose_error")

    def run(self):
        """Main request handling loop"""
        log("Bridge server ready, waiting for requests...")

        try:
            for line in sys.stdin:
                line = line.strip()
                if not line:
                    continue

                try:
                    request = json.loads(line)
                    request_type = request.get("type", "unknown")

                    if request_type == "generate":
                        self.handle_generate(request)
                    elif request_type == "health_check":
                        self.handle_health_check()
                    elif request_type == "diagnose":
                        self.handle_diagnose()
                    elif request_type == "shutdown":
                        log("Received shutdown request")
                        send_response({"type": "shutdown_ack"})
                        break
                    else:
                        send_error(f"Unknown request type: {request_type}", "unknown_request")

                except json.JSONDecodeError as e:
                    send_error(f"Invalid JSON: {str(e)}", "json_error")
                except Exception as e:
                    error_msg = f"Request handling error: {str(e)}\n{traceback.format_exc()}"
                    log(error_msg)
                    send_error(error_msg, "request_error")

        except KeyboardInterrupt:
            log("Received interrupt signal")
        except Exception as e:
            log(f"Fatal error in main loop: {str(e)}\n{traceback.format_exc()}")
            sys.exit(1)

        log("Bridge server shutting down")

def main():
    # Get model path from environment
    model_path = os.environ.get("MLX_MODEL_PATH")
    if not model_path:
        log("ERROR: MLX_MODEL_PATH environment variable not set")
        send_error("MLX_MODEL_PATH not set", "configuration_error")
        sys.exit(1)

    if not Path(model_path).exists():
        log(f"ERROR: Model path does not exist: {model_path}")
        send_error(f"Model path not found: {model_path}", "configuration_error")
        sys.exit(1)

    # Create and run server
    server = MLXBridgeServer(model_path)
    server.initialize()
    server.run()

if __name__ == "__main__":
    main()
