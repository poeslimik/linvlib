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

**가장 간단:** `tools\deploy-gui\start.bat` 을 더블클릭합니다.  
서버가 뜨고 브라우저가 http://127.0.0.1:8765 로 열립니다. (창을 닫으면 서버 종료)

또는:

```powershell
cd tools\deploy-gui
python server.py
```

## 기능

- **빌드&바이너리 / static / 제목 규칙** 체크 후 배포
- WSL `cargo build --release`
- `scp` 업로드, 바이너리 시 `systemctl stop` → 업로드 → `restart`
- SSH 테스트, 서비스 상태, journal 로그, 원격 즉시 백업
- **로컬 `.db` 업로드**: 파일명과 관계없이 서버 `linvlib.db`로 바꿔 교체. 원격 백업 → stop → 업로드 → WAL/SHM 정리 → 재시작
- **`.env` 편집**: 로컬/서버 `.env`를 메모장처럼 불러와 저장 (서버 저장 시 재시작 옵션). 내용은 로그에 남기지 않음

localhost에만 bind 됩니다. 스테이징 파일은 `tools/deploy-gui/_uploads/`에 잠시 저장됩니다.
