"""Servidor local de TTS (Kokoro-82M via mlx-audio) para o Jarvis.

Escuta só em 127.0.0.1 (porta JARVIS_TTS_PORT, padrão 7349). Não toca áudio: devolve WAV
para o cliente tocar. O modelo fica em JARVIS_TTS_MODEL_DIR (padrão: models/kokoro-82m-bf16 ao
lado deste arquivo); quem baixa é o services/tts/install.sh do repositório.

  GET  /health      -> 200 quando o modelo está carregado e aquecido
  GET  /voices      -> lista de vozes disponíveis
  POST /synthesize  -> {"text": "...", "voice": "pf_dora", "speed": 1.0} -> audio/wav
"""

import io
import json
import logging
import os
import re
import sys
import threading
import time
import wave
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

os.environ.setdefault("HF_HUB_OFFLINE", "1")

import numpy as np  # noqa: E402
from mlx_audio.tts.utils import load  # noqa: E402

HOST = "127.0.0.1"
PORT = int(os.environ.get("JARVIS_TTS_PORT") or 7349)
BASE = Path(__file__).resolve().parent
MODEL_DIR = Path(os.environ.get("JARVIS_TTS_MODEL_DIR") or BASE / "models" / "kokoro-82m-bf16")
VOICE_DIR = MODEL_DIR / "voices"
DEFAULT_VOICE = os.environ.get("JARVIS_TTS_VOICE") or "pf_dora"
LANG_CODE = "p"  # pt-br (espeak-ng via misaki)
MAX_TEXT = 4000
MAX_BODY = 64 * 1024
MAX_CHUNK = 250

logging.basicConfig(
    level=logging.INFO,
    format="%(asctime)s %(levelname)s %(message)s",
    stream=sys.stderr,
)
log = logging.getLogger("jarvis-tts")

VOICES = sorted(p.stem for p in VOICE_DIR.glob("p*.safetensors"))
MODEL = None
READY = False
GEN_LOCK = threading.Lock()


def split_text(text: str) -> str:
    """Quebra em frases curtas, uma por linha (o G2P pt-br não faz chunking)."""
    text = re.sub(r"[ \t\r\f\v]+", " ", text).strip()
    pieces = []
    for line in re.split(r"\n+", text):
        for sent in re.split(r"(?<=[.!?…;:])\s+", line.strip()):
            sent = sent.strip()
            while len(sent) > MAX_CHUNK:
                cut = max(sent.rfind(", ", 0, MAX_CHUNK), sent.rfind(" ", 0, MAX_CHUNK))
                if cut <= 0:
                    cut = MAX_CHUNK
                pieces.append(sent[:cut].strip(" ,"))
                sent = sent[cut:].strip(" ,")
            if sent:
                pieces.append(sent)
    return "\n".join(pieces)


SPOKEN_PATH = os.path.expanduser("~/.local/share/jarvis/fala.txt")


def record_spoken(text: str) -> None:
    """Grava o último texto sintetizado: a Escuta usa para não confundir a voz do Jarvis com um pedido."""
    try:
        tmp = SPOKEN_PATH + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump({"ts": time.time(), "text": text}, f, ensure_ascii=False)
        os.replace(tmp, SPOKEN_PATH)
    except OSError:
        log.warning("não gravei %s", SPOKEN_PATH)


def synthesize(text: str, voice: str, speed: float) -> bytes:
    chunks = []
    with GEN_LOCK:
        for result in MODEL.generate(
            split_text(text),
            voice=str(VOICE_DIR / f"{voice}.safetensors"),
            speed=speed,
            lang_code=LANG_CODE,
            split_pattern=r"\n+",
        ):
            chunks.append(np.asarray(result.audio, dtype=np.float32))
    if not chunks:
        raise RuntimeError("nenhum áudio gerado")
    audio = np.concatenate(chunks)
    pcm = (np.clip(audio, -1.0, 1.0) * 32767).astype("<i2")
    buf = io.BytesIO()
    with wave.open(buf, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(MODEL.sample_rate)
        w.writeframes(pcm.tobytes())
    return buf.getvalue()


class Handler(BaseHTTPRequestHandler):
    server_version = "jarvis-tts/1"

    def log_message(self, fmt, *args):
        log.info("%s %s", self.address_string(), fmt % args)

    def _json(self, code: int, payload: dict):
        body = json.dumps(payload, ensure_ascii=False).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json; charset=utf-8")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/health":
            if READY:
                self._json(200, {"status": "ok", "model": "Kokoro-82M", "voices": VOICES})
            else:
                self._json(503, {"status": "loading"})
        elif self.path == "/voices":
            self._json(200, {"voices": VOICES, "default": DEFAULT_VOICE})
        else:
            self._json(404, {"error": "not found"})

    def do_POST(self):
        if self.path != "/synthesize":
            return self._json(404, {"error": "not found"})
        if not READY:
            return self._json(503, {"error": "modelo carregando"})
        try:
            length = int(self.headers.get("Content-Length", "0"))
        except ValueError:
            length = 0
        if length <= 0 or length > MAX_BODY:
            return self._json(413 if length > MAX_BODY else 400, {"error": "corpo inválido"})
        try:
            req = json.loads(self.rfile.read(length))
        except (ValueError, UnicodeDecodeError):
            return self._json(400, {"error": "JSON inválido"})
        if not isinstance(req, dict):
            return self._json(400, {"error": "JSON inválido"})
        text = req.get("text")
        voice = req.get("voice") or DEFAULT_VOICE
        speed = req.get("speed", 1.0)
        if not isinstance(text, str) or not text.strip():
            return self._json(400, {"error": "text obrigatório"})
        if len(text) > MAX_TEXT:
            return self._json(413, {"error": f"text acima de {MAX_TEXT} caracteres"})
        if voice not in VOICES:
            return self._json(400, {"error": "voz desconhecida", "voices": VOICES})
        if not isinstance(speed, (int, float)) or not 0.5 <= speed <= 2.0:
            return self._json(400, {"error": "speed deve estar entre 0.5 e 2.0"})
        t0 = time.monotonic()
        try:
            wav = synthesize(text, voice, float(speed))
        except Exception as exc:  # noqa: BLE001
            log.exception("falha na síntese")
            return self._json(500, {"error": str(exc)})
        log.info("synth voice=%s chars=%d bytes=%d %.3fs", voice, len(text), len(wav), time.monotonic() - t0)
        record_spoken(text)
        self.send_response(200)
        self.send_header("Content-Type", "audio/wav")
        self.send_header("Content-Length", str(len(wav)))
        self.end_headers()
        self.wfile.write(wav)


def main():
    global MODEL, READY
    if not VOICES:
        sys.exit(f"nenhuma voz pt-BR em {VOICE_DIR}")
    t0 = time.monotonic()
    MODEL = load(str(MODEL_DIR))
    log.info("modelo carregado em %.2fs; vozes: %s", time.monotonic() - t0, ", ".join(VOICES))
    t0 = time.monotonic()
    for voice in VOICES:
        synthesize("Olá. Aquecendo a voz.", voice, 1.0)
    log.info("warm-up concluído em %.2fs", time.monotonic() - t0)
    READY = True
    httpd = ThreadingHTTPServer((HOST, PORT), Handler)
    httpd.daemon_threads = True
    log.info("ouvindo em http://%s:%d", HOST, PORT)
    httpd.serve_forever()


if __name__ == "__main__":
    main()
