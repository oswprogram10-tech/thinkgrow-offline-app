# 생각쑥쑥 설치형 앱

온라인 서비스와 분리된 설치형 앱 저장소다. 교사·학생·학부모는 별도 프로그램과 앱 ID를 사용한다. 관리자 설치판과 기본 제공 게임은 없다.

## 현재 구현

- Rust 저장·전송 코어: 앱/계정/기기별 SQLite partition, 암호화된 초안·제출·첨부, 고정 snapshot과 outbox 원자 commit, 자격 만료 시 자료 보존, 대상별 receipt와 재시도/충돌 상태.
- 앱별 native 계정 관리: 교사·학생·학부모에 등록·잠금·PIN 변경·보호 백업 복구/복원을 연결했다. 학생 앱에만 초안·수업·제출 명령을 둔다. 이번 Windows 계정/복구 소스 검증은 [검증 기록](docs/build-evidence/offline-core-2026-11-03.json)을 따른다.
- 온라인 변경 릴리스 연결: 변경 commit 영향 분석, 공유/게임/앱 산출물 분류, 출처·호환성·해시·서명 결속 catalog 생성, native 검증·후보 staging 및 수업/동의 gate.
- 교사 제출물 준비·기기 채점 초안·명시적 온라인 반영, 학부모 공개 보고서/공지/메시지·회신, 승인된 정적 게임의 개별 설치·수업 배정·격리 실행을 연결했다. 지원 계약과 검증 범위는 [역할별 기능·게임 연결](docs/role-domains-and-games-2026-11-03.md)을 따른다.
- Windows 교사/학생 Release 및 Android 학생/학부모 arm64 Debug 빌드. 기반 실험 32개와 코어/릴리스 검사는 아래 기록을 따른다.

Windows·Android 보호 저장소와 실제 온라인 기기 등록을 연결했다. 온라인 등록 페이지에서 학생 수업을 선택하고, 학생 앱에서 서버 서명 텍스트 수업 7유형을 준비·작성·자동저장·복원·제출 대기열에 보관할 수 있다. 설치 파일은 정식판 완성 후 공개한다.

Apple은 보호 저장소/전송 소스만 작성했으며 타깃 빌드는 미검증이다. 앱은 현재 PIN 변경·백업 암호 복구·암호화 백업/복원, 활성/과거 발급키 분리, 새 서명 자격에 의한 시각 재동기화 소스를 갖췄다. 정식 앱 패키지·실기기 복원 검수와 발급키 교체, 제출 수신 경로에서의 재결속 검증, 필수 첨부/미디어 준비, 서버/교실 게임 실행환경과 진행 전달, 정식 업데이트는 남아 있다. [현재 구현·근거·남은 순서](docs/registration-and-lessons-2026-11-03.md), [역할별 기능·게임 연결](docs/role-domains-and-games-2026-11-03.md), [자격·시각·복구 설계](docs/credential-trust-and-recovery-2026-11-03.md)를 기준으로 한다.

## 앱

| 앱 | 대상 | 앱 ID |
|---|---|---|
| 교사 | Windows | com.thinkgrow.offline.teacher |
| 학생 | Windows, Android, iPadOS | com.thinkgrow.offline.student |
| 학부모 | Android, iOS | com.thinkgrow.offline.parent |

공통 Rust 구현을 사용하더라도 앱별 명령·자격·데이터 경계를 유지한다. Android 빌드 생성물은 target/android/student, target/android/parent로 분리한다. iOS/iPadOS는 아직 빌드하지 않았다.

## 문서

- [Windows 배포·기기 등록·현재 남은 순서](docs/windows-packaging-and-registration-2026-11-03.md)

- [코어와 온라인 변경 릴리스 연결](docs/core-and-update-bridge-2026-11-03.md)
- [전송 wire 계약](contracts/README.md)
- [1단계 검수](docs/phase-1-validation.md), [기반 실험](docs/foundation-validation-2026-11-03.md)
- [Android 빌드 방법](docs/android-build.md)
- [코어 검사 기록](docs/build-evidence/offline-core-2026-11-03.json), [릴리스 통합 검사](docs/build-evidence/release-bridge-2026-11-03.json)

## 다음 구현 순서

1. 보호 백업·PIN 복구·타기기 재결속을 실제 앱에서 검수하고, 공개키 선배포 후 실제 발급키 교체 절차와 제출 수신 경로의 epoch fencing을 검증한다. 재부팅 시계 되돌림은 읽기·복구 모드로 보존하며 새 epoch 자격으로 동기화한다.
2. 연결한 서명 텍스트 수업/학생 7유형 작성에 필수 첨부·미디어·게임 자산 준비를 확장한다.
3. 실제 교실/온라인 전송 어댑터와 서버 중복 수신 장부, 취소·충돌 검토를 연결한다.
4. 연결한 교사·학부모 흐름을 확장하고 서버/교실 게임 실행환경과 게임 진행의 교사 전송을 연결한다. 기존 모든 게임의 오프라인 호환을 완료로 표시하지 않는다.
5. 실제 앱 서명·피드 조회·플랫폼 업데이트/복구를 통합한다. macOS/Xcode 환경에서는 Apple 타깃을 별도로 빌드한다.

제품 화면·실기기·운영 연결 증거가 없는 범위를 완료로 표시하지 않는다.
