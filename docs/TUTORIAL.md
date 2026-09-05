# 첫 로그인 튜토리얼

이메일 인증(또는 즉시 토큰 가입) 직후 한 번, 앱 사용법을 iframe 미리보기로 안내합니다.  
완료 여부는 DB에 저장하지 않습니다. 다시 보기는 탑바 **튜토리얼** 버튼입니다.

## 사용자 흐름

1. 가입 → 인증 메일 링크 → `/verify?token=…`  
2. 성공 시 `/series?tour=1` 로 이동  
3. 작품 목록이 `tour=1`을 기억한 뒤 `startTour()` 실행  
4. 건너뛰기 / 시작하기 → 모달만 닫힘 (서버 기록 없음)  
5. 이후: 탑바 **튜토리얼** 또는 `/series?tour=1`

즉시 로그인 토큰이 나오는 가입도 `/series?tour=1`로 보냅니다.  
일반 재로그인은 튜토리얼을 자동으로 띄우지 않습니다.

## 4단계 미리보기

| 단계 | iframe | 패치 |
|------|--------|------|
| 1 작품 찾기 | `/series?sort=popular&ps_in=complete` | 진행률 0% |
| 2 읽음 기록 | `/series/{티어문 id}` | `GET /api/v1/tour/demo` 의 읽음·평가 |
| 3 요청 | `/import` | DB의 「인생 역전」+ 방법1/2 라벨 (알라딘 검색 아님) |
| 4 티어리스트 | `/tierlist` | `tour/demo` 의 티어 배치 |

미리보기 안은 클릭 불가, 스크롤만 가능합니다.  
상단 이메일은 `example@linvlib.cloud` 로 표시합니다.

## 프론트 코드

| 파일 | 역할 |
|------|------|
| `static/js/tour.js` | 모달·iframe·단계 패치 (`startTour`) |
| `static/css/app.css` | `.tour-*`, `#tour-root` |
| `static/js/pages/verify.js` | 인증 후 `?tour=1` |
| `static/js/pages/home.js` | 즉시가입·`next` 쿼리 |
| `static/js/pages/series-list.js` | `wantTour` → `startTour` |
| `static/js/ui.js` | 탑바 튜토리얼 버튼 |
| `static/js/router.js` | 미로그인 시 `login?next=…` 보존 |
| `static/js/api.js` | `tourDemo()` |

iframe 안 SPA에서는 투어를 다시 열지 않습니다 (`isTourPreviewFrame`).

## 백엔드

| 파일 | 역할 |
|------|------|
| `GET /api/v1/tour/demo` | 로그인 필요. 데모 계정 실시간 스냅샷 |
| `src/services/tour.rs` | `iskim070208@gmail.com` + 티어문 시리즈 |
| `src/handlers/tour.rs` | HTTP |
| `src/services/tierlist.rs` | `get_tierlist_snapshot` (시드 쓰기 없음) |
| `TourDemoResponse` | `models` |

데모 계정·시리즈 ID를 바꾸려면 `services/tour.rs` 상수와 `static/js/tour.js`의 `TEARMOON_ID`를 함께 맞춥니다.

## 마이그레이션

- `019_tutorial_completed_at` / `020_drop_tutorial_completed_at`  
  한때 완료 시각을 저장했다가 제거한 이력입니다. **현재 로직은 이 컬럼을 쓰지 않습니다.**
