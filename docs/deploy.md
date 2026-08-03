# 서버 배포 치트시트

클라우드 VM에 linvlib를 올리고 재시작할 때 자주 쓰는 명령 모음입니다.  
아래 값은 **예시**이므로 본인 환경에 맞게 바꾸세요.

| 항목 | 값 (예시) |
|------|-----------|
| 공인 IP / 호스트 | `YOUR_SERVER_IP` |
| 도메인 | `https://example.com` |
| SSH 사용자 | `ubuntu` |
| SSH 키 | `C:\Users\YOU\.ssh\your-key.key` |
| 서버 배포 경로 | `~/linvlib` (`/home/ubuntu/linvlib`) |
| 앱 포트 | `3000` (nginx가 80/443 → 3000 프록시) |

PowerShell에서는 매번 이렇게 두고 쓰면 편합니다.

```powershell
$KEY = "C:\Users\YOU\.ssh\your-key.key"
$IP  = "YOUR_SERVER_IP"
```

---

## 1. 리눅스 바이너리 빌드 (WSL Ubuntu)

Windows `.exe`가 아니라 **확장자 없는** `linvlib`을 올립니다. WSL에서 빌드하세요.

```bash
cd /mnt/c/Users/YOU/Documents/GitHub/linvlib   # 본인 클론 경로로
source "$HOME/.cargo/env"   # rustup 설치 직 필요할 때만
cargo build --release
ls -lh target/release/linvlib
file target/release/linvlib   # ELF 64-bit 이어야 함
```

산출물 경로 (Windows에서도 동일 파일):

`C:\Users\YOU\Documents\GitHub\linvlib\target\release\linvlib`

---

## 2. SSH 접속

```powershell
ssh -i $KEY ubuntu@${IP}
```

---

## 3. 파일 업로드 (PowerShell → 서버)

### 배포 폴더가 없을 때 (최초 1회)

서버(SSH)에서:

```bash
mkdir -p ~/linvlib/config
```

### 바이너리만 (Rust 로직 변경)

실행 중이면 덮어쓰기가 실패할 수 있으니, 막히면 먼저 중지:

```bash
# SSH
sudo systemctl stop linvlib
```

```powershell
scp -i $KEY target\release\linvlib ubuntu@${IP}:~/linvlib/
```

```bash
# SSH
chmod +x ~/linvlib/linvlib
sudo systemctl start linvlib
# 또는: sudo systemctl restart linvlib
```

### static만 (프론트 HTML/CSS/JS)

```powershell
scp -i $KEY -r static ubuntu@${IP}:~/linvlib/
```

서비스 재시작은 보통 필요 없습니다. 브라우저에서 **Ctrl+F5**로 캐시를 비우세요.

특정 파일만:

```powershell
scp -i $KEY static\js\pages\tierlist.js ubuntu@${IP}:~/linvlib/static/js/pages/
```

### 제목 규칙만

```powershell
scp -i $KEY config\title_rules.toml ubuntu@${IP}:~/linvlib/config/
```

```bash
# SSH — 규칙 파일은 기동 시 읽히므로 재시작
sudo systemctl restart linvlib
```

### 한 번에 (코드+프론트 배포)

```powershell
scp -i $KEY target\release\linvlib ubuntu@${IP}:~/linvlib/
scp -i $KEY -r static ubuntu@${IP}:~/linvlib/
scp -i $KEY config\title_rules.toml ubuntu@${IP}:~/linvlib/config/
```

```bash
# SSH
chmod +x ~/linvlib/linvlib
sudo systemctl restart linvlib
```

### `.env` 수정

비밀값이므로 Git에 올리지 마세요. 앱은 **기동 시** `.env`를 읽으므로, 바꾼 뒤에는 반드시 재시작합니다.

#### A) 서버에서 직접 수정 (권장)

```bash
# SSH
cd ~/linvlib
ls -la .env
nano .env          # 또는: vim .env
```

`nano` 사용법: 수정 → `Ctrl+O` Enter(저장) → `Ctrl+X`(종료)

```bash
sudo systemctl restart linvlib
sudo journalctl -u linvlib -n 30 --no-pager   # 기동 오류 확인
```

한 줄만 바꿀 때 예 (`EMAIL_DEV_MODE` 끄기):

```bash
cd ~/linvlib
grep EMAIL_DEV_MODE .env || echo 'EMAIL_DEV_MODE=false' >> .env
sed -i 's/^EMAIL_DEV_MODE=.*/EMAIL_DEV_MODE=false/' .env
grep EMAIL_DEV_MODE .env
sudo systemctl restart linvlib
```

`APP_BASE_URL`을 도메인으로 맞추기:

```bash
cd ~/linvlib
grep APP_BASE_URL .env || echo 'APP_BASE_URL=https://linvlib.cloud' >> .env
sed -i 's|^APP_BASE_URL=.*|APP_BASE_URL=https://linvlib.cloud|' .env
grep APP_BASE_URL .env
sudo systemctl restart linvlib
```

#### B) PC에서 고쳐 올린 뒤 덮어쓰기

로컬 `.env`를 서버용으로 맞춘 다음:

```powershell
scp -i $KEY .env ubuntu@${IP}:~/linvlib/.env
```

```bash
# SSH
sudo systemctl restart linvlib
```

로컬용(`APP_BASE_URL=http://localhost:3000`, `EMAIL_DEV_MODE=true` 등)을 그대로 올리면 **운영 동작이 깨질 수 있으니** 서버 값을 확인하세요.

#### 서버 `.env`에서 자주 보는 항목

```env
DATABASE_URL=sqlite:linvlib.db
JWT_SECRET=긴-임의-문자열
JWT_EXPIRY_HOURS=168
ALADIN_TTB_KEY=ttb...
BIND_ADDR=0.0.0.0:3000
ADMIN_EMAIL=관리자@example.com
APP_BASE_URL=https://linvlib.cloud
EMAIL_DEV_MODE=false
SMTP_HOST=smtp.gmail.com
SMTP_PORT=587
SMTP_USERNAME=...
SMTP_PASSWORD=...
SMTP_FROM=linvlib <noreply@example.com>
RUST_LOG=info,linvlib=info
```

| 변수 | 의미 |
|------|------|
| `APP_BASE_URL` | 인증 메일 링크의 공개 주소. 서버는 `https://linvlib.cloud` |
| `EMAIL_DEV_MODE` | `true`면 가입 화면에 인증 토큰 링크 노출(개발용). **운영은 `false`** |
| `ADMIN_EMAIL` | 이 주소로 가입 시 관리자 + 이메일 인증 생략 |
| `BIND_ADDR` | 앱 리슨 주소. nginx 뒤에서 `0.0.0.0:3000` |
| `SMTP_*` | 인증 메일 발송. 비어 있으면 사실상 dev 모드처럼 동작할 수 있음 |

systemd 유닛에 `WorkingDirectory=/home/ubuntu/linvlib`가 있어야 `.env`를 그 경로에서 읽습니다. 기동이 이상하면 유닛을 확인하세요.

```bash
systemctl cat linvlib
```

---

## 4. 서비스 제어 (SSH)

```bash
cd ~/linvlib

sudo systemctl status linvlib --no-pager
sudo systemctl restart linvlib
sudo systemctl stop linvlib
sudo systemctl start linvlib

# 로그
sudo journalctl -u linvlib -n 80 --no-pager
sudo journalctl -u linvlib -f
```

systemd 유닛이 없다면:

```bash
cd ~/linvlib
pkill -f './linvlib' || true
nohup ./linvlib > linvlib.log 2>&1 &
```

nginx (설정 바꾼 뒤에만):

```bash
sudo nginx -t
sudo systemctl reload nginx
sudo systemctl status nginx --no-pager
```

---

## 5. 동작 확인

```bash
# SSH — 앱 직접
curl -sI http://127.0.0.1:3000/health

# SSH — nginx 경유
curl -sI http://127.0.0.1/health
curl -sI https://linvlib.cloud/health
```

```powershell
# PC
curl.exe -sI https://linvlib.cloud/health
```

배포 후 프론트가 안 바뀌면 **Ctrl+F5**.

---

## 6. 업로드 실패 (`dest open … Failure`)

`scp: dest open "linvlib/linvlib": Failure` 가 나오면 서버 쪽 문제입니다.

```bash
# SSH
ls -la ~/linvlib
df -h ~
mkdir -p ~/linvlib/config
sudo chown -R ubuntu:ubuntu ~/linvlib
sudo systemctl stop linvlib   # 바이너리 잠김/권한 이슈 완화
```

그다음 PC에서 `scp`를 다시 실행하고 `systemctl start linvlib` 합니다.

---

## 7. 무엇을 올릴지 빠른 판단

| 변경 내용 | 올릴 것 | 재시작 |
|-----------|---------|--------|
| `src/**` (Rust) | `linvlib` 바이너리 | `systemctl restart linvlib` |
| `static/**` | `static/` | 보통 불필요 (Ctrl+F5) |
| `config/title_rules.toml` | 해당 파일 | 재시작 |
| `.env` | `.env` | 재시작 |
| DB 스키마(마이그레이션) | 바이너리 (마이그레이션 내장) | 재시작 — DB 파일은 백업 후 유지 |

`migrations/`·소스·`target/` 전체는 서버에 올릴 필요 없습니다.

---

## 8. DB 백업

앱이 **실행 중에도** 백업할 수 있습니다. 내부적으로 SQLite `VACUUM INTO`로 일관된 스냅샷을 `backups/`에 저장합니다.

| 항목 | 값 |
|------|-----|
| 자동 시각 | 매일 **KST 자정** (날짜가 바뀌면 1회) |
| 보관 | **14일** (`BACKUP_RETAIN_DAYS`, 기본 14) |
| 디렉터리 | `~/linvlib/backups` (`BACKUP_DIR`, 기본 `backups`) |
| 관리 UI | 관리 탭 → **백업** (목록·지금 백업·다운로드·삭제) |

선택 env (서버 `.env`):

```env
BACKUP_DIR=backups
BACKUP_RETAIN_DAYS=14
```

수동(앱 중지 **불필요**):

```bash
# SSH — 앱이 켜져 있어도 됨. 관리 UI 「지금 백업」과 동일 계열
cd ~/linvlib
# 또는 관리자 API / 웹 UI 사용
```

`sqlite3` CLI로 할 때(온라인):

```bash
cd ~/linvlib
mkdir -p backups
stamp=$(TZ=Asia/Seoul date +%Y%m%d-%H%M%S)
sqlite3 linvlib.db ".backup 'backups/linvlib-$stamp.db'"
```

`cp`로 복사할 때만 앱을 잠시 멈추는 편이 안전합니다(WAL).

PC로 받기: 관리 탭 다운로드, 또는

```powershell
scp -i $KEY ubuntu@${IP}:~/linvlib/backups/linvlib-YYYYMMDD-HHMMSS.db .
```

---

## 9. 로컬 배포 GUI (빌드·업로드·재시작)

터미널 대신 PC 브라우저에서 배포하려면:

```powershell
cd tools\deploy-gui
copy config.example.json config.json
# config.json 에서 키·IP·repo 경로 확인
python server.py
```

http://127.0.0.1:8765 에서 **바이너리 / static / 제목 규칙**을 고른 뒤 배포·재시작·상태·로그·원격 백업을 실행할 수 있습니다.  
자세한 설명은 `tools/deploy-gui/README.md`를 보세요.
