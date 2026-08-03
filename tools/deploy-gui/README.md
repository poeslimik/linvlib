# linvlib 로컬 배포 GUI

PC에서만 도는 작은 웹 UI입니다. 서버(linvlib)에 포함되지 않습니다.

## 준비

1. OpenSSH 클라이언트 (`ssh`, `scp`)와 WSL Ubuntu + Rust 툴체인
2. 설정 파일 만들기:

```powershell
cd tools\deploy-gui
copy config.example.json config.json
notepad config.json
```

`ssh_key`, `ssh_host`, `repo_dir` 등을 본인 환경에 맞게 수정합니다.  
`config.json`은 비밀 경로가 들어갈 수 있어 Git에 올리지 마세요.

## 실행

```powershell
python server.py
```

브라우저에서 http://127.0.0.1:8765 를 엽니다.

## 기능

- **빌드&바이너리 / static / 제목 규칙** 체크 후 배포
- WSL `cargo build --release`
- `scp` 업로드, 바이너리 시 `systemctl stop` → 업로드 → `restart`
- SSH 테스트, 서비스 상태, journal 로그, 원격 즉시 백업

localhost에만 bind 됩니다.
