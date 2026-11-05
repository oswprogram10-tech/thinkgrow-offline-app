# 생각쑥쑥 앱 디자인 시스템 (2026-11-05)

이 문서는 웹 교사·학생 앱, 공개 페이지, 운영자 관리 콘솔(thinkgrow-admin), 오프라인 앱(thinkgrow-offline-app)이 함께 따르는 기준이다. 같은 문서와 `tokens.json`이 세 저장소의 `docs/design/`에 똑같이 들어 있다. 값을 바꿀 때는 세 곳을 함께 바꾼다.

## 1. 방향
- 기준 화면: 공개 홈 `cloud/site/index.html`(검정 바탕, 초록 `#38c98a` 알약 버튼, 호박색 라벨, 큰 글씨와 넉넉한 여백, 상자·테두리를 줄인 구성).
- 앱은 오래 읽고 쓰는 작업 화면이라 **기본은 밝은 화면**, 사용자가 고르면 **어두운 화면**. 홈의 분위기는 어두운 화면과 강조색·버튼 모양·글씨 위계로 이어 간다.
- 지금 화면이 "난잡하다"는 것이 출발점이다. 색만 바꾸지 말고, 한 화면에서 해야 할 일 하나가 먼저 보이도록 정리한다.

## 2. 토큰
- 이름과 값은 `tokens.json`이 정본이다. CSS에는 아래 형태로 그대로 옮긴다.
  ```css
  :root{ /* fixed 값 전부 + themed 의 light 값 */ color-scheme:light; }
  :root[data-theme="dark"]{ /* themed 의 dark 값 */ color-scheme:dark; }
  ```
- 화면 CSS는 색·글씨 크기·간격·모서리를 **토큰으로만** 쓴다. 새 16진수 색을 쓰지 않는다(게임·외부 자료·인쇄 전용 규칙은 예외, 그 경우 주석으로 이유를 단다).
- 예전 변수(`--ink`, `--green`, `--primary`, `--bg-main` 등)는 지우지 말고 새 토큰을 가리키게 바꾼다: `--green: var(--tg-accent-text);`. 남아 있는 코드가 깨지지 않게 하기 위해서다.

## 3. 밝게·어둡게 전환
- 저장 위치: `localStorage["thinkgrow-theme"]`, 값은 `"light"` 또는 `"dark"`. 값이 없거나 읽기에 실패하면 `"light"`(시스템 설정은 따르지 않는다).
- 적용: `<html data-theme="light|dark">`. 화면 깜박임을 막으려고 `<head>` 안, 스타일시트 바로 뒤에서 동기 스크립트로 먼저 적용한다. `localStorage` 접근은 `try/catch`로 감싼다.
- 전환 버튼: `<button id="theme-toggle" type="button" aria-pressed="true|false">`(어두운 화면일 때 `true`). 글자는 밝을 때 "어두운 화면", 어두울 때 "밝은 화면". 머리글의 사용자 영역(로그아웃 버튼 옆)에 둔다. 로그인 전 화면에서도 보여야 한다.
- 전환할 때 `transition`은 배경·글자색에만 짧게(.2s). `prefers-reduced-motion: reduce`면 없음.

## 4. 구성 요소 규칙
| 요소 | 규칙 |
|---|---|
| 기본 버튼(화면마다 가장 중요한 행동 1개) | 알약: 배경 `--tg-accent`, 글자 `--tg-on-accent`, `--tg-radius-pill`, 높이 44px 이상, 글자 14~16px 600 |
| 보조 버튼 | 배경 없음, 테두리 1px `--tg-line-strong`, 같은 알약 모양 |
| 텍스트 버튼·링크 | `--tg-accent-text`, 밑줄은 hover·focus 때 |
| 위험 행동 | 글자·테두리 `--tg-danger`, 채운 빨강 버튼은 확인 단계에서만 |
| 입력칸 | 높이 44px 이상, 테두리 `--tg-line-strong`, 모서리 `--tg-radius-sm`, 라벨은 입력칸 위 |
| 카드·패널 | 배경 `--tg-surface`, 테두리 1px `--tg-line`, 모서리 `--tg-radius`, 그림자 `--tg-shadow`. 카드 안에 카드를 겹치지 않는다 |
| 작은 라벨(분류·상태 머리말) | 12~14px 600, `--tg-spark-text` 또는 `--tg-text-muted` |
| 알림 줄(`#notice` 등) | 성공 `--tg-accent-soft`, 경고 `--tg-warning-soft`, 오류 `--tg-danger-soft` 배경 + 같은 계열 글자 |
| 표 | 줄무늬 없음, 행 구분선 `--tg-line`, 머리행 `--tg-text-muted` 14px |
| 초점 | 모든 상호작용 요소 `:focus-visible{outline:2px solid var(--tg-focus);outline-offset:2px}` 하나로 통일 |

## 5. 배치 원칙
- 한 화면에 기본 버튼은 하나. 나머지는 보조·텍스트 버튼.
- 화면 제목(24~32px) → 한 줄 설명(`--tg-text-muted`) → 주요 내용 순. 설명 문단이 세 줄을 넘으면 접거나 줄인다.
- 본문 최대 폭 1200px, 좌우 여백 24px(폭 700px 이하에서 16px). 가로 스크롤이 생기면 안 된다(넓은 표는 표만 가로 스크롤).
- 간격은 `--tg-space-*`만. 요소 사이 12~18px, 구역 사이 36~60px.
- 터치 대상은 44×44px 이상(학생은 태블릿을 많이 쓴다).
- 글자는 `word-break: keep-all`.

## 6. 바꾸면 안 되는 것 (모든 저장소 공통)
- 시험과 스크립트가 찾는 **id, class, data- 속성, 버튼·제목 문구**. 각 작업 카드에 목록이 있다. 모양만 바꾸고 이름은 그대로 둔다.
- 화면의 동작·데이터 흐름·API 호출. 이번 작업은 겉모습과 배치만 다룬다.
- 외부 글꼴·CDN을 새로 불러오지 않는다(오프라인 수업에서도 같은 모습이어야 한다). 글꼴은 `--tg-font`의 시스템 글꼴 순서로 쓴다.

## 7. 완료 증거: 화면 사진
작업 카드마다 정해진 화면을 **밝게·어둡게 × 폭 1280px·390px**로 찍어 카드가 지정한 폴더에 PNG로 남긴다. 파일 이름 `<화면>-<light|dark>-<1280|390>.png`. 합성 데이터만 쓰고 실제 학생·교사 정보가 찍히면 안 된다.
