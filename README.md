# linvlib

라이트노벨 **읽기 기록 · 평가 · 티어리스트**를 위한 작은 웹 앱입니다.  
작품 메타데이터는 **예스24 Open API**(검색·신간·표지)에서 가져옵니다.

> 공급자·가져오기·신간: **[docs/CATALOG.md](docs/CATALOG.md)**  
> 구조 개요: **[docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)** · 메일: [docs/EMAIL.md](docs/EMAIL.md) · 튜토리얼: [docs/TUTORIAL.md](docs/TUTORIAL.md)

## 무엇을 하나요?


| 역할         | 할 수 있는 일                                                                        |
| ---------- | ------------------------------------------------------------------------------- |
| **일반 사용자** | 회원가입·로그인, 작품 검색/필터, 권 단위 읽음, 평가(S~F), 티어리스트, 카탈로그 **추가·수정·검색 개선·기타** 요청         |
| **관리자**    | 도서 가져오기 / 직접 등록, 요청 승인·거절, **신간 갱신**, 미등록 신간 **추천**, 검색 별칭·작품 묶음, 시리즈 합병·분리, 백업 |


## 기술 스택

- **Backend:** Rust, Axum, SQLx (SQLite)
- **Frontend:** 바닐라 JS SPA (`static/`) — 빌드 도구 없음
- **배포:** systemd + nginx (선택), 로컬 배포 GUI (`tools/deploy-gui/`)



## 5분 안에 로컬 실행

```bash
cp .env.example .env
# JWT_SECRET, YES24_API_KEY 필수. ADMIN_EMAIL이면 그 계정이 관리자.
cargo run
```

브라우저: [http://localhost:3000](http://localhost:3000)

- DB 파일은 기본적으로 `linvlib.db`
- `migrations/` 는 **서버 기동 시 자동 적용** (이미 적용된 SQL 파일은 내용을 바꾸지 마세요)

```bash
cargo test
```



## 디렉터리 한눈에

```
linvlib/
├── src/                 # Rust API 서버
│   ├── main.rs          # 기동, 마이그레이션, 스케줄러
│   ├── routes/          # HTTP 라우트
│   ├── handlers/        # 요청 진입점
│   ├── services/        # 비즈니스 로직 (catalog, 신간 …)
│   ├── repositories/    # SQL
│   └── models/          # 요청/응답 DTO
├── static/              # 프론트 SPA (js/pages/*.js)
├── migrations/          # DB 스키마 버전 (001 → …)
├── config/              # title_rules.toml 등
├── docs/                # 설계·배포 문서
└── tools/deploy-gui/    # Windows용 배포 콘솔
```

자세한 모듈 설명은 [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## 핵심 개념



### 카탈로그 vs 개인 기록

- **시리즈·권**은 전역 카탈로그 (공유)
- **읽음·평가·티어**는 사용자별



### 가져오기 검색

로그인 사용자의 `/import` 검색·등록은 **예스24** Goods API를 사용합니다. → [docs/CATALOG.md](docs/CATALOG.md)

### 신간 갱신 (전량 검색 아님)

매일 **23:30 KST**(또는 관리자 「지금 신간 갱신」)에:

1. 예스24 LN 카테고리 신상품·임프린트 최근 출간을 가져옴
2. 카탈로그에 이미 있는 작품만 권 목록을 갱신
3. 목록에 있지만 카탈로그에 **없는** LN 후보는 관리 **추천** 탭에 쌓음

흐름 상세: [docs/ARCHITECTURE.md § 신간 갱신](docs/ARCHITECTURE.md#신간-갱신).

### 검색 별칭 · 작품 묶음

- **별칭:** 「전생슬」처럼 줄임말로 검색  
- **묶음:** 본편·외전처럼 제목이 달라도 한쪽을 검색하면 함께 표시



### 연재 현황 (`publish_status`)


| 값                  | 표시           |
| ------------------ | ------------ |
| `ongoing`          | 연재중          |
| `complete_partial` | 완결(번역 미완)    |
| `complete`         | 완결           |
| `complete_stalled` | 완결(번역 중단)    |
| `ongoing_stalled`  | 연재중(번역 중단)   |
| `hiatus`           | 연재 중단(번역 미완) |
| `hiatus_done`      | 연재 중단        |




## 환경 변수


| 변수                                  | 설명                    | 기본                                  |
| ----------------------------------- | --------------------- | ----------------------------------- |
| `DATABASE_URL`                      | SQLite URL            | `sqlite:linvlib.db`                 |
| `JWT_SECRET`                        | JWT 서명 비밀키            | **필수**                              |
| `JWT_EXPIRY_HOURS`                  | 토큰 유효 시간              | `168`                               |
| `YES24_API_KEY`                     | 예스24 Open API 키 (`X-Api-Key`) | **필수**                         |
|                                      | 호출 한도: 초당 10 · 일 soft 19,500 / hard 20,000 (KST) | |

| `ADMIN_EMAIL`                       | 부트스트랩 관리자 이메일         | (없으면 없음)                            |
| `APP_BASE_URL`                      | 메일 링크용 공개 URL         | `http://localhost:3000`             |
| `BIND_ADDR`                         | listen 주소             | `0.0.0.0:3000`                      |
| `BACKUP_DIR` / `BACKUP_RETAIN_DAYS` | DB 백업                 | `backups` / `14`                    |
| `DISCORD_STATUS_WEBHOOK_URL`        | 신간 갱신 직후 일일 상태 보고   | (선택)                                |
| `TITLE_RULES_PATH`                  | 제목 규칙                 | `config/title_rules.toml`           |
| `EMAIL_DEV_MODE`                    | 인증 토큰을 API에 포함        | SMTP 없으면 `true`                     |
| `SMTP_*`                            | 인증·비밀번호 재설정 메일        | (선택)                                |
| `RUST_LOG`                          | 로그 필터                 | `info,linvlib=info,tower_http=info` |


전체 예시는 `.env.example`.

## 스케줄 (KST)


| 시각        | 작업                        |
| --------- | ------------------------- |
| **23:30** | 신간 목록 기반 갱신·추천 (예스24). 끝나면 Discord 일일 상태 (웹훅 설정 시) |
| **00:00** | SQLite DB 백업              |


스케줄 날짜 계산은 KST 기준입니다.

## API (자주 쓰는 것)

인증: `Authorization: Bearer <token>`


| Method          | Path                                           | 설명                        |
| --------------- | ---------------------------------------------- | ------------------------- |
| GET             | `/health`                                      | 헬스                        |
| POST            | `/api/v1/auth/register` · `login` · `verify` … | 계정                        |
| GET             | `/api/v1/series` · `/series/{id}`              | 목록·상세                     |
| PUT             | `/api/v1/series/{id}/reads` · `/rating`        | 읽음·평가                     |
| GET/POST        | `/api/v1/catalog-requests`                     | 사용자 요청                    |
| GET             | `/api/v1/imports/search`                       | 도서 검색 예스24 (로그인)             |
| POST            | `/api/v1/imports`                              | 가져오기 (**관리자**)            |
| POST            | `/api/v1/admin/refresh`                        | 신간 갱신 시작 (백그라운드, **관리자**) |
| GET/POST/DELETE | `/api/v1/admin/new-releases…`                  | 신간 추천 (**관리자**)           |
| GET/PUT         | `/api/v1/tierlist`                             | 티어리스트                     |


나머지는 코드를 보는 편이 빠릅니다: `src/routes/mod.rs`.

## 배포

운영 서버에 올리는 절차·치트시트: **[docs/deploy.md](docs/deploy.md)**  
Windows에서는 `tools/deploy-gui/start.bat` 로 빌드·업로드·재시작을 할 수 있습니다.

### 마이그레이션 주의

- 새 스키마는 `migrations/0xx_….sql` **새 파일**로만 추가  
- **이미 적용된 파일 수정 금지** → `VersionMismatch` 로 서버가 안 뜹니다 (502)



## 문서 목록


| 문서                                                       | 내용                |
| -------------------------------------------------------- | ----------------- |
| [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)             | 구조, 데이터 흐름, 모듈 지도 |
| [docs/CATALOG.md](docs/CATALOG.md)                       | 도서 공급자, 가져오기·신간   |
| [docs/deploy.md](docs/deploy.md)                         | 서버 배포·백업·트러블슈팅    |
| [docs/EMAIL.md](docs/EMAIL.md)                           | 인증·재설정 메일         |
| [docs/TUTORIAL.md](docs/TUTORIAL.md)                     | 첫 로그인 튜토리얼        |
| [tools/deploy-gui/README.md](tools/deploy-gui/README.md) | 로컬 배포 GUI         |


