# 트랜잭션 이메일

인증·비밀번호 재설정 메일은 **파일 템플릿**으로 관리합니다.

## 템플릿 위치

| 용도 | HTML | 텍스트 |
|------|------|--------|
| 가입 인증 | `templates/email/verify.html` | `templates/email/verify.txt` |
| 비밀번호 재설정 | `templates/email/reset.html` | `templates/email/reset.txt` |

플레이스홀더:

- `{{VERIFY_URL}}` / `{{RESET_URL}}` — 버튼·링크 URL  
- `{{EXPIRES_HOURS}}` — 유효 시간(시간)

렌더링: `src/services/email.rs`에서 `include_str!` + 문자열 치환 후 SMTP multipart(HTML + plain) 발송.

## 관련 코드

- `services/email.rs` — 가입·인증·재설정 메일, 미인증 계정 정리 스케줄  
- `main.rs` — `email::spawn_unverified_user_cleanup` (기동 직후 + 주기)  
- 프론트: `static/js/pages/verify.js`, `reset-password.js`, `home.js`

## 환경변수

SMTP·발신 주소 등은 `.env` / [README](../README.md) 환경변수 표를 따릅니다.  
OCI Email Delivery는 **발송 전용**이며 수신함은 제공하지 않습니다.

## 미리보기

로컬에서 HTML만 보려면 템플릿 파일을 브라우저로 열고 플레이스홀더를 임시 URL로 바꾸면 됩니다.  
시안용 별도 `docs/email-mockups` 폴더는 유지하지 않습니다.
