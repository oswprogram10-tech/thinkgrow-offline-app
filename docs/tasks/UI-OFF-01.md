# UI-OFF-01 오프라인 앱(교사·학생·보호자) 디자인 통일

## 목표
세 설치 앱의 화면을 공통 디자인 시스템(`docs/design/design-system.md`, `docs/design/tokens.json`)에 맞추고 밝게·어둡게 전환을 넣는다. 수업 중 인터넷 없이 써야 하므로 외부 파일은 하나도 불러오지 않는다. design-system.md를 먼저 끝까지 읽는다.

## 현재 상태
- 앱별 화면: `apps/teacher/ui/index.html`, `apps/student/ui/index.html`, `apps/parent/ui/index.html`(정적 한 페이지, 구역을 `hidden`으로 전환). 1~38행은 세 앱이 거의 같다.
- 공유 코드: `apps/shared-ui/app.css`(압축된 24줄), `app.js`, `roles.js`, `materials.js`, `games.js`. 학생 전용 `apps/student/ui/student.js`.
- 빌드: `scripts/build-ui.mjs <teacher|student|parent>`가 `target/ui/<role>/`로 복사. **앱별 `apps/*/ui/app.css`는 복사되지 않는다**(죽은 파일).
- 색은 하드코딩(`#17212b`, `#f5f7f8`, `#dce3e7`, `#225d52`, `#9d251b`), 변수 없음. 폭 700px 이하 규칙 하나.
- 머리말 색을 앱마다 다르게 하려던 흔적(교사 `#225d52`, 학생 `#4266a8`, 보호자 `#8a5837`)이 죽은 파일에만 있다.

## 구현 결정
1. `apps/shared-ui/app.css`를 읽기 쉽게 풀어서 다시 쓴다. 맨 위에 design-system.md 2절 형식의 토큰 전부.
2. 앱 구분색(`--tg-role-accent`)을 `app.css`에 둔다(tokens.json에는 넣지 않는다):
   - `body[data-role="teacher"]`: light `#127a4c` / dark `#5fdca5`
   - `body[data-role="student"]`: light `#2f5ea8` / dark `#8fb3ff`
   - `body[data-role="parent"]`: light `#8a5837` / dark `#e0a87a`
   `.eyebrow`와 작업 영역 머리의 얇은 강조선에만 쓴다. 기본 버튼은 세 앱 모두 `--tg-accent`.
3. 구성 요소는 design-system.md 4절. `.security-settings`(`<details>`)는 접힌 보조 영역으로 보이게(작은 글자, `--tg-text-muted`). 잠금 해제·등록 폼은 화면 가운데 폭 420px 카드 하나.
4. 학생 수업 화면(`.lesson-layout`, `#lesson-editor`, `.answer-choice`, `.order-row`): 본문 18px, 줄 간격 1.7, 선택지·버튼 높이 48px 이상. 1280px에서 목록(왼쪽 320px)·편집기 두 열, 700px 이하 한 열.
5. 밝게·어둡게:
   - 새 `apps/shared-ui/theme.js`: 즉시 실행으로 `localStorage["thinkgrow-theme"]`(없으면 `"light"`)를 `<html data-theme>`에 적용, `DOMContentLoaded`에서 `#theme-toggle` 연결(design-system.md 3절). try/catch.
   - `scripts/build-ui.mjs`가 세 앱 모두에 `theme.js`를 복사하게 한다.
   - 세 `index.html`의 `<head>`에서 `app.css` 링크 바로 다음에 `<script src="theme.js"></script>`. 머리글 오른쪽에 `<button id="theme-toggle" class="theme-toggle" type="button" aria-pressed="false">어두운 화면</button>`.
   - 세 앱은 저장 공간이 서로 다르므로 선택도 앱마다 따로 저장된다. 이것은 의도한 동작이다.
6. "개발 중" 배지, 문구, 구역 순서는 그대로. 죽은 `apps/*/ui/app.css`는 지우지 말고 REPORT에 "쓰이지 않음"으로 적는다.

## 건드리지 말 것
- AGENTS.md: 세 앱은 계속 따로(식별자·자격·저장 공간 분리). 관리자 설치 앱을 만들지 않는다. 수업 중 네트워크가 필요한 것을 넣지 않는다.
- 시험·공유 JS가 찾는 id: `#notice`(문구 "확인하고 있습니다" 포함), `#unlock-form`, `#unlock-pin`, `#register-form`, `#get-code`, `#recover-pin-form`, `#restore-backup-form`, `#workspace`, `#access-state`, `#local-summary`, `#lock`, `#role-title`, `#materials-section`, `#games-section`, `#lesson-list`, `#lesson-title`, `#lesson-prompt`, `#lesson-editor`, `#answer-form`, `#answer-text`, `#answer-fields`, `#answer-controls`, `#choice-N`, `#match-N`, `#blank-N`, `.order-row`, `#prepare-lessons`, `#lesson-notice`, `#role-list`, `#role-editor`, `#role-save`, `#role-send`, `#role-notice`, `#grade-feedback`, `#grade-confirm`, `#parent-message`, `#game-catalog`, `#game-install`, 그리고 `#unlock-form`·`#register-form` 안의 `fieldset`.
- `app.js`, `roles.js`, `materials.js`, `games.js`, `student.js`의 동작. 이 카드에서 JS는 `theme.js` 추가와 `build-ui.mjs`의 복사 목록만 고친다.
- Rust 코드(`crates/`, `apps/*/src-tauri/`), 키·서명·배포 설정, 운영 서버 호출, 푸시.

## 테스트
이 저장소 AGENTS.md는 요청이 있을 때만 시험을 돌리게 한다. **이 카드는 시험 실행을 요청한다.**
- `node scripts/build-ui.mjs teacher`, `student`, `parent`
- `node tools/verify_account_ui.mjs`, `node tools/verify_student_ui.mjs`, `node tools/verify_role_ui.mjs` (Edge 준비가 안 되면 고치지 말고 보고)
- 새 `tools/verify_design_tokens.mjs`: `docs/design/tokens.json` 값이 `apps/shared-ui/app.css`의 `:root`(fixed + light)와 `:root[data-theme="dark"]`(dark)에 같은 값으로 있는지, `target/ui/<role>/theme.js`가 세 앱 모두에 있는지 확인.
- 화면 사진: 새 `tools/capture_ui_screens.mjs`(기존 `tools/verify_*.mjs`처럼 Edge CDP + 합성 데이터). 화면: 앱마다 `unlock`, `register`, `workspace`, 학생은 추가로 `lesson-editor`. 결과 `docs/ui-screens/UI-OFF-01/<role>-<화면>-<light|dark>-<1280|390>.png`. 작업 영역까지 갈 수 없는 화면이 있으면 찍을 수 있는 것만 찍고 이유를 보고.

## 완료 확인
- [ ] 빌드 3개, 시험 4개 결과 보고
- [ ] `docs/ui-screens/UI-OFF-01/`에 사진(최대 40장)
- [ ] `app.css`에 tokens.json 밖의 16진수 색은 `--tg-role-accent` 6개뿐
- [ ] 외부 URL(`http`)을 불러오는 코드 없음
- [ ] 커밋 1개
