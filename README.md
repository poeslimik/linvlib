# linvlib

Light novel Library — 라이트노벨 읽기 기록·평가·티어리스트

## 주요 기능

- 회원가입, 이메일 인증·재발송, 로그인(JWT), 회원 탈퇴
- 이용약관·개인정보처리방침 동의 (`/terms`, `/privacy`)
- 작품 목록·검색·필터, 권 단위 읽음, 평가(S~F)
- 알라딘 검색·시리즈 가져오기(관리자) / 카탈로그 추가·수정·삭제 요청(일반)
- 수동 시리즈 등록·편집·삭제(관리자)
- 알라딘 API 일일 쿼터, **매일 자정(KST)** 자동 신간 갱신
- 티어리스트(드래그, 이미지 저장·복사)
- 관리자 콘솔(요청 심사, 직접 등록 작품, 쿼터·갱신 상태, 사용자)

도서 메타데이터 출처: [알라딘 인터넷서점](https://www.aladin.co.kr) Open API

## 기술 스택

- Backend: Rust, Axum, SQLx (SQLite)
- Frontend: 바닐라 JS SPA (`static/`)

## 요구 사항

- Rust 1.75+
- [알라딘 Open API](https://blog.aladin.co.kr/openapi) TTBKey
- (선택) SMTP — 없으면 `EMAIL_DEV_MODE`로 인증 토큰을 응답/로그에 표시

## 빠른 시작

```bash
cp .env.example .env
# .env에 JWT_SECRET, ALADIN_TTB_KEY 등을 채웁니다
cargo run
```

기본 주소: `http://localhost:3000`

`ADMIN_EMAIL`에 지정한 주소로 가입하면 관리자·이메일 인증 완료로 처리됩니다.  
서버 기동 시 해당 이메일이 이미 있으면 `is_admin`을 부여합니다.

## 환경 변수

| 변수 | 설명 | 기본 |
|------|------|------|
| `DATABASE_URL` | SQLite URL | `sqlite:linvlib.db` |
| `JWT_SECRET` | JWT 서명 비밀키 | (필수) |
| `JWT_EXPIRY_HOURS` | 토큰 유효 시간(시간) | `168` |
| `ALADIN_TTB_KEY` | 알라딘 TTBKey | (필수) |
| `ALADIN_DAILY_QUOTA` | 일일 API 호출 상한 | `5000` |
| `ALADIN_SOFT_QUOTA` | 자동 갱신 중단 임계 | `4800` |
| `ADMIN_EMAIL` | 부트스트랩 관리자 이메일 | (비어 있으면 없음) |
| `APP_BASE_URL` | 인증 메일 링크용 공개 URL | `http://localhost:3000` |
| `TITLE_RULES_PATH` | 제목 정규화 규칙 파일 | `config/title_rules.toml` |
| `BIND_ADDR` | 서버 bind 주소 | `0.0.0.0:3000` |
| `EMAIL_DEV_MODE` | `true`면 인증 토큰을 API 응답에 포함 | SMTP 미설정 시 `true` |
| `SMTP_HOST` / `SMTP_PORT` / `SMTP_USERNAME` / `SMTP_PASSWORD` / `SMTP_FROM` | 인증 메일 발송 | (선택) |
| `RUST_LOG` | 로그 필터 | `info,linvlib=info,tower_http=info` |

## 설정 파일

`config/title_rules.toml` — 시리즈 제목 정규화, 합본·한정판 제외, 아크/권수 파싱 규칙

## API 요약

| Method | Path | 설명 |
|--------|------|------|
| GET | `/health` | 헬스 체크 |
| POST | `/api/v1/auth/register` | 회원가입 (약관 동의·이메일 인증) |
| POST | `/api/v1/auth/resend-verification` | 인증 메일 재발송 |
| POST | `/api/v1/auth/verify` | 이메일 인증 |
| POST | `/api/v1/auth/login` | 로그인 |
| GET | `/api/v1/auth/me` | 내 정보 |
| DELETE | `/api/v1/auth/me` | 회원 탈퇴 |
| GET | `/api/v1/series` | 작품 목록 |
| GET | `/api/v1/series/{id}` | 작품 상세 |
| POST/PUT/DELETE | `/api/v1/series…` | 수동 시리즈 CRUD (**관리자**) |
| PUT | `/api/v1/series/{id}/reads` | 읽음 저장 |
| PUT | `/api/v1/series/{id}/rating` | 평가 저장 |
| GET | `/api/v1/imports/search` | 알라딘 검색 (**인증**) |
| POST | `/api/v1/imports` | 시리즈 가져오기 (**관리자**) |
| GET/POST | `/api/v1/catalog-requests` | 내 요청 목록 / 요청 생성 |
| GET/PUT | `/api/v1/admin/catalog-requests…` | 요청 심사 (**관리자**) |
| GET | `/api/v1/admin/status` | 쿼터·갱신 상태 (**관리자**) |
| GET | `/api/v1/admin/manual-series` | 직접 등록 작품 목록 (**관리자**) |
| GET | `/api/v1/admin/users` | 사용자 목록 (**관리자**) |
| POST | `/api/v1/admin/refresh` | 일괄 신간 갱신 (**관리자**) |
| GET/PUT | `/api/v1/tierlist` | 티어리스트 |

인증이 필요한 API는 `Authorization: Bearer <token>` 헤더를 사용합니다.

## 개발

```bash
cargo test
cargo run
```

마이그레이션은 `migrations/`에 있으며 서버 기동 시 자동 적용됩니다.

## 서버 배포

OCI에 올리거나 재배포할 때 쓰는 명령은 [docs/deploy.md](docs/deploy.md)를 보세요.

## 알라딘 Open API 유의사항

- 등록 URL은 서비스 공개 주소(예: `https://linvlib.cloud`)로 맞춥니다.
- 이용 조건·메타데이터 저장 범위는 [알라딘 OpenAPI 안내](https://blog.aladin.co.kr/openapi)와 운영자 회신을 따릅니다.
- 가져오기는 국내 **종이책 라이트노벨**과 **LN 계열 전자책**(시프트노벨 등)을 검색합니다. 만화/코믹 전자책이나 알라딘에 등록되지 않은 작품은 검색되지 않을 수 있으며, 그 경우 관리자 수동 등록을 사용합니다.
