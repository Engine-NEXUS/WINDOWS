#!/usr/bin/env python3
"""
NEXUS NLU Server — FastAPI server providing intent classification + slot filling.

Loads a BERT-Mini ONNX model and provides a /parse endpoint that takes text
and returns {intent, slots, confidence}.

Lazy-started by the Rust NLU client (nlu_client.rs) on port 39218.

Usage:
  python nlu_server.py
  curl -X POST http://127.0.0.1:39218/parse -H "Content-Type: application/json" -d '{"text":"open whatsapp"}'
"""

import json
import os
import sys
import time
from pathlib import Path

import numpy as np
import onnxruntime as ort
import uvicorn
from fastapi import FastAPI
from pydantic import BaseModel

# ─── Config ────────────────────────────────────────────────────────────────

PORT = 39218
MODEL_DIR = Path(__file__).parent / "nlu" / "model"
ONNX_PATH = MODEL_DIR / "nexus_nlu.onnx"
TOKENIZER_DIR = MODEL_DIR / "tokenizer"

INTENTS = [
    "open_app",
    "analyse_repo",
    "analyse_pr",
    "search",
    "open_architect",
    "media_control",
    "unknown",
]
ID_TO_INTENT = {i: intent for i, intent in enumerate(INTENTS)}

SLOT_TYPES = [
    "O",
    "B-app_name", "I-app_name",
    "B-repo", "I-repo",
    "B-owner", "I-owner",
    "B-pr_number", "I-pr_number",
    "B-query", "I-query",
    "B-media_action", "I-media_action",
]
ID_TO_SLOT = {i: slot for i, slot in enumerate(SLOT_TYPES)}

MAX_LEN = 64

# ─── App ───────────────────────────────────────────────────────────────────

app = FastAPI(title="NEXUS NLU Server")

_session: ort.InferenceSession | None = None
_tokenizer = None


def get_session() -> ort.InferenceSession:
    global _session
    if _session is None:
        print(f"[NLU] Loading ONNX model from {ONNX_PATH}")
        _session = ort.InferenceSession(str(ONNX_PATH))
        print(f"[NLU] Model loaded: {_session.get_providers()}")
    return _session


def get_tokenizer():
    global _tokenizer
    if _tokenizer is None:
        from transformers import AutoTokenizer
        print(f"[NLU] Loading tokenizer from {TOKENIZER_DIR}")
        _tokenizer = AutoTokenizer.from_pretrained(str(TOKENIZER_DIR))
    return _tokenizer


class ParseRequest(BaseModel):
    text: str


class ParseResponse(BaseModel):
    intent: str
    slots: dict
    confidence: float
    latency_ms: float


@app.get("/health")
async def health():
    return {"status": "ok", "model_loaded": _session is not None}


@app.post("/parse")
async def parse(req: ParseRequest) -> ParseResponse:
    start = time.time()

    if _session is None or _tokenizer is None:
        return ParseResponse(
            intent="unknown",
            slots={},
            confidence=0.0,
            latency_ms=0.0,
        )

    # Tokenize
    encoding = _tokenizer(
        req.text,
        truncation=True,
        padding="max_length",
        max_length=MAX_LEN,
        return_tensors="np",
    )

    input_ids = encoding["input_ids"].astype(np.int64)
    attention_mask = encoding["attention_mask"].astype(np.int64)

    # Inference
    session = get_session()
    outputs = session.run(
        None,
        {"input_ids": input_ids, "attention_mask": attention_mask},
    )

    intent_logits = outputs[0][0]  # (num_intents,)
    slot_logits = outputs[1][0]  # (seq_len, num_slots)

    # Intent prediction
    intent_id = int(np.argmax(intent_logits))
    intent_probs = _softmax(intent_logits)
    confidence = float(intent_probs[intent_id])
    intent = ID_TO_INTENT[intent_id]

    # Slot extraction (BIO decoding)
    slots = extract_slots(slot_logits, input_ids[0])

    latency_ms = (time.time() - start) * 1000
    return ParseResponse(
        intent=intent,
        slots=slots,
        confidence=confidence,
        latency_ms=latency_ms,
    )


def _softmax(x):
    e = np.exp(x - np.max(x))
    return e / e.sum()


def extract_slots(slot_logits, input_ids):
    """Decode BIO tags into slot dict."""
    pred_ids = np.argmax(slot_logits, axis=-1)

    # Get token texts
    tokenizer = get_tokenizer()
    tokens = tokenizer.convert_ids_to_tokens(input_ids.tolist())

    # Extract spans
    slots = {}
    current_slot = None
    current_parts = []  # list of (text, is_subword) tuples

    for i, (tag_id, token) in enumerate(zip(pred_ids, tokens)):
        if token in ["[CLS]", "[SEP]", "[PAD]"]:
            if current_slot and current_parts:
                slots[current_slot] = _join_parts(current_parts)
            current_slot = None
            current_parts = []
            continue

        tag = ID_TO_SLOT[int(tag_id)]
        is_subword = token.startswith("##")
        clean_token = token.replace("##", "")

        if tag.startswith("B-"):
            if current_slot and current_parts:
                slots[current_slot] = _join_parts(current_parts)
            current_slot = tag[2:]
            current_parts = [(clean_token, is_subword)]
        elif tag.startswith("I-") and current_slot == tag[2:]:
            current_parts.append((clean_token, is_subword))
        else:
            if current_slot and current_parts:
                slots[current_slot] = _join_parts(current_parts)
            current_slot = None
            current_parts = []

    # Don't forget the last span
    if current_slot and current_parts:
        slots[current_slot] = _join_parts(current_parts)

    # Clean up slot values
    for key in list(slots.keys()):
        val = slots[key].strip()
        if not val:
            del slots[key]
        else:
            slots[key] = val

    return slots


def _join_parts(parts):
    """Join subword token parts into a single string.
    
    Subword tokens (## prefix) are joined without spaces.
    Regular tokens are joined with spaces.
    """
    result = ""
    for text, is_subword in parts:
        if is_subword:
            result += text  # no space for subword continuations
        else:
            if result:
                result += " "
            result += text
    return result


def main():
    # Check if model exists
    if not ONNX_PATH.exists():
        print(f"[NLU] ERROR: ONNX model not found at {ONNX_PATH}")
        print("[NLU] Run train.py first to train and export the model.")
        sys.exit(1)

    # Pre-load model and tokenizer
    print("[NLU] Pre-loading model and tokenizer...")
    get_session()
    get_tokenizer()
    print(f"[NLU] Ready. Listening on port {PORT}")

    uvicorn.run(app, host="127.0.0.1", port=PORT, log_level="warning")


if __name__ == "__main__":
    main()
