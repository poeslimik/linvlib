#!/usr/bin/env python3
"""Local-only deploy console for linvlib (build / upload / restart).

Usage (Windows):
  cd tools\\deploy-gui
  copy config.example.json config.json   # edit paths/IP/key
  python server.py
  # open http://127.0.0.1:8765
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from urllib.parse import urlparse

HERE = Path(__file__).resolve().parent
CONFIG_PATH = HERE / "config.json"
LOG_LOCK = threading.Lock()
LOG_LINES: list[str] = []
JOB_LOCK = threading.Lock()
JOB_RUNNING = False


def load_config() -> dict:
    if not CONFIG_PATH.exists():
        example = HERE / "config.example.json"
        raise SystemExit(
            f"Missing {CONFIG_PATH.name}. Copy {example.name} → config.json and edit it."
        )
    with CONFIG_PATH.open(encoding="utf-8") as f:
        cfg = json.load(f)
    required = ["repo_dir", "ssh_key", "ssh_host", "ssh_user", "remote_dir", "service_name"]
    for key in required:
        if not cfg.get(key):
            raise SystemExit(f"config.json missing `{key}`")
    cfg.setdefault("wsl_distro", "Ubuntu")
    cfg.setdefault("bind", "127.0.0.1")
    cfg.setdefault("port", 8765)
    return cfg


CFG = load_config()


def log(msg: str) -> None:
    line = f"[{time.strftime('%H:%M:%S')}] {msg}"
    with LOG_LOCK:
        LOG_LINES.append(line)
        del LOG_LINES[:-400]
    print(line, flush=True)


def win_to_wsl(path: str) -> str:
    p = Path(path).resolve()
    drive = p.drive.rstrip(":").lower()
    rest = str(p).replace("\\", "/")
    if ":" in rest:
        rest = rest.split(":", 1)[1]
    return f"/mnt/{drive}{rest}"


def run(cmd: list[str], cwd: str | None = None, timeout: int | None = None) -> int:
    log("$ " + " ".join(cmd))
    try:
        proc = subprocess.Popen(
            cmd,
            cwd=cwd,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            text=True,
            encoding="utf-8",
            errors="replace",
        )
        assert proc.stdout is not None
        for line in proc.stdout:
            log(line.rstrip())
        return proc.wait(timeout=timeout)
    except Exception as exc:  # noqa: BLE001
        log(f"ERROR: {exc}")
        return 1


def ssh_base() -> list[str]:
    return [
        "ssh",
        "-i",
        CFG["ssh_key"],
        "-o",
        "StrictHostKeyChecking=accept-new",
        f"{CFG['ssh_user']}@{CFG['ssh_host']}",
    ]


def scp_base() -> list[str]:
    return [
        "scp",
        "-i",
        CFG["ssh_key"],
        "-o",
        "StrictHostKeyChecking=accept-new",
    ]


def remote(cmd: str) -> int:
    return run(ssh_base() + [cmd])


def do_build() -> int:
    repo = CFG["repo_dir"]
    wsl_repo = win_to_wsl(repo)
    distro = CFG.get("wsl_distro") or "Ubuntu"
    script = (
        f'source "$HOME/.cargo/env" 2>/dev/null; '
        f"cd '{wsl_repo}' && cargo build --release && "
        f"file target/release/linvlib && ls -lh target/release/linvlib"
    )
    return run(["wsl", "-d", distro, "--", "bash", "-lc", script])


def do_upload(parts: list[str], stop_first: bool) -> int:
    repo = Path(CFG["repo_dir"])
    remote_dir = CFG["remote_dir"]
    target = f"{CFG['ssh_user']}@{CFG['ssh_host']}"

    if stop_first and "binary" in parts:
        if remote(f"sudo systemctl stop {CFG['service_name']}") != 0:
            log("warn: stop failed (continuing)")

    if "binary" in parts:
        src = repo / "target" / "release" / "linvlib"
        if not src.exists():
            log(f"missing binary: {src}")
            return 1
        code = run(scp_base() + [str(src), f"{target}:{remote_dir}/linvlib"])
        if code != 0:
            return code
        if remote(f"chmod +x {remote_dir}/linvlib") != 0:
            return 1

    if "static" in parts:
        src = repo / "static"
        code = run(scp_base() + ["-r", str(src), f"{target}:{remote_dir}/"])
        if code != 0:
            return code

    if "title_rules" in parts:
        src = repo / "config" / "title_rules.toml"
        if not src.exists():
            log(f"missing: {src}")
            return 1
        remote(f"mkdir -p {remote_dir}/config")
        code = run(scp_base() + [str(src), f"{target}:{remote_dir}/config/title_rules.toml"])
        if code != 0:
            return code

    return 0


def do_restart() -> int:
    return remote(f"sudo systemctl restart {CFG['service_name']}")


def do_status() -> int:
    return remote(
        f"sudo systemctl status {CFG['service_name']} --no-pager -l | head -n 40; "
        f"echo '---'; date; "
        f"ls -lh {CFG['remote_dir']}/linvlib {CFG['remote_dir']}/linvlib.db 2>/dev/null; "
        f"ls -lh {CFG['remote_dir']}/backups 2>/dev/null | tail -n 8"
    )


def do_logs() -> int:
    return remote(f"sudo journalctl -u {CFG['service_name']} -n 80 --no-pager")


def do_backup_now() -> int:
    # Prefer admin API later; for now invoke sqlite VACUUM via remote if app running is fine —
    # call a tiny remote script using sqlite3 .backup without stopping.
    return remote(
        f"cd {CFG['remote_dir']} && mkdir -p backups && "
        f"stamp=$(TZ=Asia/Seoul date +%Y%m%d-%H%M%S) && "
        f"sqlite3 linvlib.db \".backup 'backups/linvlib-$stamp.db'\" && "
        f"ls -lh backups/linvlib-$stamp.db"
    )


def run_job(action: str, payload: dict) -> None:
    global JOB_RUNNING
    with JOB_LOCK:
        if JOB_RUNNING:
            log("busy: another job is running")
            return
        JOB_RUNNING = True
    try:
        log(f"=== start: {action} ===")
        code = 0
        if action == "build":
            code = do_build()
        elif action == "upload":
            parts = payload.get("parts") or []
            code = do_upload(parts, stop_first=bool(payload.get("stop_first", True)))
        elif action == "restart":
            code = do_restart()
        elif action == "deploy":
            parts = payload.get("parts") or []
            if payload.get("build"):
                code = do_build()
            if code == 0 and parts:
                code = do_upload(parts, stop_first=bool(payload.get("stop_first", True)))
            if code == 0 and payload.get("restart", True):
                needs_restart = "binary" in parts or "title_rules" in parts or payload.get("force_restart")
                if needs_restart or payload.get("restart"):
                    code = do_restart()
        elif action == "status":
            code = do_status()
        elif action == "logs":
            code = do_logs()
        elif action == "backup_remote":
            code = do_backup_now()
        elif action == "ssh_test":
            code = remote("echo ok && hostname && uname -a")
        else:
            log(f"unknown action: {action}")
            code = 1
        log(f"=== done: {action} (exit {code}) ===")
    finally:
        with JOB_LOCK:
            JOB_RUNNING = False


INDEX_HTML = r"""<!DOCTYPE html>
<html lang="ko">
<head>
  <meta charset="UTF-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>linvlib 배포 콘솔</title>
  <style>
    :root {
      --bg: #eef2f5;
      --ink: #1c2430;
      --muted: #5c6b7a;
      --line: #c5d0db;
      --panel: #fff;
      --accent: #1f7a74;
      --danger: #b33b3b;
    }
    * { box-sizing: border-box; }
    body {
      margin: 0;
      font-family: "Segoe UI", "Noto Sans KR", sans-serif;
      color: var(--ink);
      background: var(--bg);
    }
    .wrap { max-width: 920px; margin: 0 auto; padding: 1.25rem 1rem 2.5rem; }
    h1 { margin: 0 0 0.35rem; font-size: 1.45rem; }
    .lead { margin: 0 0 1rem; color: var(--muted); font-size: 0.95rem; }
    .banner {
      background: #fff8e8; border: 1px solid #e6d3a0; color: #6a5520;
      border-radius: 10px; padding: 0.7rem 0.9rem; margin-bottom: 1rem; font-size: 0.9rem;
    }
    .panel {
      background: var(--panel); border: 1px solid var(--line); border-radius: 12px;
      padding: 1rem; margin-bottom: 0.85rem;
    }
    .panel h2 { margin: 0 0 0.75rem; font-size: 1.05rem; }
    .checks { display: flex; flex-wrap: wrap; gap: 0.75rem 1.25rem; margin-bottom: 0.85rem; }
    label { display: inline-flex; align-items: center; gap: 0.4rem; cursor: pointer; }
    .row { display: flex; flex-wrap: wrap; gap: 0.45rem; }
    button {
      border: 1px solid var(--line); background: #f7fafb; color: var(--ink);
      border-radius: 8px; padding: 0.5rem 0.85rem; font: inherit; cursor: pointer;
    }
    button:hover { border-color: var(--accent); }
    button.primary { background: var(--accent); color: #fff; border-color: var(--accent); }
    button.danger { background: #fff; color: var(--danger); border-color: #e0b0b0; }
    button:disabled { opacity: 0.55; cursor: wait; }
    #log {
      width: 100%; height: 360px; resize: vertical; font-family: Consolas, monospace;
      font-size: 0.82rem; background: #15202b; color: #d7e2ec; border-radius: 10px;
      border: 1px solid #0f1720; padding: 0.75rem; white-space: pre-wrap;
    }
    .meta { color: var(--muted); font-size: 0.85rem; margin-top: 0.5rem; }
  </style>
</head>
<body>
  <div class="wrap">
    <div class="banner">로컬(127.0.0.1) 전용입니다. SSH 키·서버 IP는 <code>config.json</code>에서 읽습니다. 프로덕션 앱과 무관한 PC 도구입니다.</div>
    <h1>linvlib 배포 콘솔</h1>
    <p class="lead">빌드 · 선택 업로드 · 재시작을 버튼으로 실행합니다.</p>

    <section class="panel">
      <h2>배포 대상</h2>
      <div class="checks">
        <label><input type="checkbox" id="part-binary" checked /> 빌드&amp;바이너리</label>
        <label><input type="checkbox" id="part-static" checked /> static</label>
        <label><input type="checkbox" id="part-rules" /> 제목 규칙</label>
        <label><input type="checkbox" id="opt-build" checked /> 업로드 전 WSL 빌드</label>
        <label><input type="checkbox" id="opt-restart" checked /> 업로드 후 재시작</label>
        <label><input type="checkbox" id="opt-stop" checked /> 바이너리 업로드 전 stop</label>
      </div>
      <div class="row">
        <button class="primary" id="btn-deploy">선택 항목 배포</button>
        <button id="btn-build">빌드만</button>
        <button id="btn-upload">업로드만</button>
        <button id="btn-restart">재시작만</button>
      </div>
    </section>

    <section class="panel">
      <h2>서버 유틸</h2>
      <div class="row">
        <button id="btn-ssh">SSH 연결 테스트</button>
        <button id="btn-status">상태 보기</button>
        <button id="btn-logs">로그 (최근 80줄)</button>
        <button id="btn-backup">원격 즉시 백업</button>
        <button class="danger" id="btn-clear">로그 지우기</button>
      </div>
      <p class="meta" id="cfg-meta">설정 로딩…</p>
    </section>

    <section class="panel">
      <h2>실행 로그</h2>
      <pre id="log"></pre>
    </section>
  </div>
  <script>
    const logEl = document.getElementById("log");
    let busy = false;

    function parts() {
      const out = [];
      if (document.getElementById("part-binary").checked) out.push("binary");
      if (document.getElementById("part-static").checked) out.push("static");
      if (document.getElementById("part-rules").checked) out.push("title_rules");
      return out;
    }

    async function refreshLog() {
      const res = await fetch("/api/log");
      const data = await res.json();
      logEl.textContent = (data.lines || []).join("\n");
      logEl.scrollTop = logEl.scrollHeight;
      busy = !!data.busy;
      document.querySelectorAll("button").forEach((b) => {
        if (b.id !== "btn-clear") b.disabled = busy;
      });
    }

    async function post(action, payload = {}) {
      await fetch("/api/run", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ action, ...payload }),
      });
      await refreshLog();
    }

    document.getElementById("btn-deploy").onclick = () =>
      post("deploy", {
        parts: parts(),
        build: document.getElementById("opt-build").checked,
        restart: document.getElementById("opt-restart").checked,
        stop_first: document.getElementById("opt-stop").checked,
      });
    document.getElementById("btn-build").onclick = () => post("build");
    document.getElementById("btn-upload").onclick = () =>
      post("upload", { parts: parts(), stop_first: document.getElementById("opt-stop").checked });
    document.getElementById("btn-restart").onclick = () => post("restart");
    document.getElementById("btn-ssh").onclick = () => post("ssh_test");
    document.getElementById("btn-status").onclick = () => post("status");
    document.getElementById("btn-logs").onclick = () => post("logs");
    document.getElementById("btn-backup").onclick = () => post("backup_remote");
    document.getElementById("btn-clear").onclick = async () => {
      await fetch("/api/log", { method: "DELETE" });
      await refreshLog();
    };

    fetch("/api/config").then((r) => r.json()).then((c) => {
      document.getElementById("cfg-meta").textContent =
        `${c.ssh_user}@${c.ssh_host} · ${c.remote_dir} · service=${c.service_name} · repo=${c.repo_dir}`;
    });
    setInterval(refreshLog, 1000);
    refreshLog();
  </script>
</body>
</html>
"""


class Handler(BaseHTTPRequestHandler):
    server_version = "linvlib-deploy-gui/1.0"

    def log_message(self, fmt: str, *args) -> None:  # quieter
        return

    def _send(self, code: int, body: bytes, content_type: str) -> None:
        self.send_response(code)
        self.send_header("Content-Type", content_type)
        self.send_header("Content-Length", str(len(body)))
        self.send_header("Cache-Control", "no-store")
        self.end_headers()
        self.wfile.write(body)

    def _json(self, code: int, obj: dict) -> None:
        raw = json.dumps(obj, ensure_ascii=False).encode("utf-8")
        self._send(code, raw, "application/json; charset=utf-8")

    def do_GET(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path in ("/", "/index.html"):
            self._send(200, INDEX_HTML.encode("utf-8"), "text/html; charset=utf-8")
            return
        if path == "/api/log":
            with LOG_LOCK:
                lines = list(LOG_LINES)
            with JOB_LOCK:
                busy = JOB_RUNNING
            self._json(200, {"lines": lines, "busy": busy})
            return
        if path == "/api/config":
            self._json(
                200,
                {
                    "repo_dir": CFG["repo_dir"],
                    "ssh_host": CFG["ssh_host"],
                    "ssh_user": CFG["ssh_user"],
                    "remote_dir": CFG["remote_dir"],
                    "service_name": CFG["service_name"],
                    "wsl_distro": CFG.get("wsl_distro"),
                },
            )
            return
        self._json(404, {"error": "not found"})

    def do_DELETE(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path == "/api/log":
            with LOG_LOCK:
                LOG_LINES.clear()
            self._json(200, {"ok": True})
            return
        self._json(404, {"error": "not found"})

    def do_POST(self) -> None:  # noqa: N802
        path = urlparse(self.path).path
        if path != "/api/run":
            self._json(404, {"error": "not found"})
            return
        length = int(self.headers.get("Content-Length") or 0)
        raw = self.rfile.read(length) if length else b"{}"
        try:
            payload = json.loads(raw.decode("utf-8") or "{}")
        except json.JSONDecodeError:
            self._json(400, {"error": "invalid json"})
            return
        action = payload.get("action")
        if not action:
            self._json(400, {"error": "action required"})
            return
        threading.Thread(target=run_job, args=(action, payload), daemon=True).start()
        self._json(202, {"ok": True, "action": action})


def main() -> None:
    bind = CFG.get("bind") or "127.0.0.1"
    if bind not in ("127.0.0.1", "localhost", "::1"):
        raise SystemExit("Refusing to bind outside localhost. Edit config bind.")
    port = int(CFG.get("port") or 8765)
    if not shutil.which("ssh") or not shutil.which("scp"):
        log("warn: ssh/scp not found on PATH (install OpenSSH client)")
    if os.name == "nt" and not shutil.which("wsl"):
        log("warn: wsl not found — build will fail until WSL is available")
    httpd = ThreadingHTTPServer((bind, port), Handler)
    log(f"deploy GUI listening on http://{bind}:{port}")
    log(f"config: {CONFIG_PATH}")
    try:
        httpd.serve_forever()
    except KeyboardInterrupt:
        log("bye")


if __name__ == "__main__":
    main()
