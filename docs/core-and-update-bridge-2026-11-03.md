# 오프라인 코어와 온라인 변경 릴리스 연결 — 2026-11-03

> 이 문서의 등록·UI 미연결 설명은 초기 코어 통합 시점의 기록이다. 실제 등록·수업 연결과 후속 시각/PIN 구현은 [등록 진행 기록](registration-and-lessons-2026-11-03.md) 및 [자격 신뢰·복구 설계](credential-trust-and-recovery-2026-11-03.md)를 기준으로 한다.

## 현재 구현

| 영역 | 구현 위치 | 동작 |
|---|---|---|
| 버전 고정 wire 계약 | contracts, crates/offline-core/src/contract.rs | 자격·초안·불변 제출·기기 proof·목적지별 receipt. Python↔Rust 한글/이모지 fixture |
| 로컬 저장 | crates/offline-core/src/store.rs | 앱/계정/기기 partition, SQLite WAL/FULL, revision CAS·이력, 고정 제출+outbox 원자 commit |
| 내용 보호 | crypto.rs | AES-256-GCM, OS RNG nonce, partition/table/record AAD. 본문·제출·충돌 내용·첨부 암호화 |
| 전송 코어 | transport.rs | 영속 lease, 목적지별 재시도·backoff/jitter·Retry-After, 인증 대기, 충돌 보존, 검증된 receipt만 완료 |
| 네이티브 경계 | runtime.rs, 앱별 src-tauri | 고정 역할 상태. 학생만 load/save/queue 명령. 교사·학부모는 현재 상태 명령만. 등록 전 fail closed |
| 릴리스 후보 검증 | update.rs | 고정 공개키 Minisign, 카탈로그 sequence/time/channel/source, 앱/플랫폼/버전/protocol/schema 검사, 스트리밍 해시·서명, 수업/동의 gate |
| 온라인 변경 분석·릴리스 생성 | tools/release_bridge.py | 온라인 commit diff 분류, 리뷰 필요 표시, committed/clean 설치판 출처 고정, 역할별 파일+서명+catalog 생성 |
| 자동화 설정 | .github/workflows | 설치판 코어 CI, 온라인 commit 알림 수신·영향 분석. 온라인 송신 workflow는 별도 template |

기본 게임 0개, 교사/학생/학부모 별도 프로그램, 관리자 설치판 없음이라는 결정을 유지한다.

## 온라인 코드를 바꾸면 어떻게 반영하는가

1. 온라인 main 변경을 repository_dispatch로 설치판 저장소에 알린다. 원본 저장소는 고정하며 입력은 전체 commit SHA로 제한한다.
2. bridge.json의 기준 commit 이후 변경 경로를 분석한다. 공통 문항/규칙은 공유 패키지와 설치판 재빌드 대상으로, 게임은 개별 게임 모듈 릴리스 대상으로, 분류되지 않은 코드는 수동 검토 대상으로 표시한다.
3. 온라인 전용 인증·관리자·클라우드 기능은 설치판 역할과 로컬 실행 환경에 맞게 반영한다. 웹 소스 변경을 곧바로 설치판에 실행시키거나 Python을 학생 모바일 앱에 자동 이식하지 않는다.
4. 검토된 설치판 source commit에서 앱/공유 패키지/게임 산출물을 만든다. 게임 ZIP 내부의 기존 G1 서명·권한·dependency·라이선스 규격은 기존 승인 설치기가 계속 검사한다.
5. release_bridge prepare가 파일 SHA-256·크기·Tauri 호환 서명과 앱 ID/플랫폼/판본/최소 코어/protocol/schema 범위를 catalog에 묶어 서명한다. 출력 디렉터리는 덮어쓰지 않는다.
6. HTTPS 배포 위치에 게시한 뒤, 앱의 native download adapter가 내려받은 파일을 Ledger에 전달한다. Ledger는 검증한 후보만 준비 영역에 둔다. 수업 종료와 사용자 확인 후 플랫폼 설치기에 넘길 수 있다.
7. 사용자 DB와 설치/모듈 버전 영역을 분리한다. 현재 코어는 다운로드 후보를 검증·준비하며 실제 OS 설치/health rollback을 수행하지 않는다. Windows Tauri updater, Android package manager/Play, Apple 배포 경로는 각각 통합해야 한다.

따라서 온라인 변경은 재사용·호환 검토와 서명 릴리스를 거쳐 업데이트로 반영할 수 있다. 관리자·클라우드 전용 기능이 설치판에서 자동 실행되는 구조는 아니다.

## 지금 활성화되지 않은 연결

사용자는 2026-11-03에 설치판 GitHub 저장소가 아직 없다고 확인했다. 로컬 저장소도 아직 초기 commit과 origin이 없다. CI 파일을 GitHub에 올리거나 실행하지 않았다. 원격 설정·서명키·피드 없이 성공한 자동 배포로 표시하지 않는다.

| 필요한 연결 | 현재 상태 |
|---|---|
| 설치판 GitHub 저장소/초기 commit | 로컬 구현만 존재. 원격 미연결 |
| 온라인 알림 대상 | 온라인 repo에 template 배치와 OFFLINE_REPOSITORY, OFFLINE_DISPATCH_TOKEN 설정 필요 |
| 비공개 온라인 소스 읽기 | 필요 시 설치판 ONLINE_SOURCE_READ_TOKEN 설정 |
| 앱 릴리스용 개인키·고정 공개키 | 미설정. 이번 검사는 임시 합성 키만 사용. 기존 게임 키를 변경하지 않음 |
| HTTPS catalog/signature/artifact 위치 | 미설정. prepare는 로컬 파일 생성만 수행하고 업로드하지 않음 |
| 실제 native 피드 조회/다운로드·플랫폼 설치기 | 후속 통합. 현재 local file staging API와 검사 CLI 제공 |
| 실제 기기 등록/자격 발급·수신 API | 기존 온라인에 새 /api/offline/v1 계약 연결 필요. 현재 transport trait은 실제 서버 주소나 receipt를 가정하지 않음 |
| OS vault / 모바일 adapter | NativeAccount에 vault key를 넘기는 seam 제공. 실제 등록·키 보관·잠금 해제 어댑터는 후속 |
| 공통 7종 문항·평가/renderer 패키지 | 이번 저장/전송 계약과 별개. 원천 추출·소비 연결은 후속 |

## 저장·인증 경계

본문과 첨부는 암호화되지만 SQLite 전체 파일 암호화는 아니다. account/device ID, schema, 전달 상태 등 metadata는 app-private DB에 남는다. DataKey와 기기 SigningKey는 DB에 저장하지 않으며 native account adapter가 OS vault에서 제공해야 한다. WebView는 키·issuer 공개키·임의 SQL·전체 파일 경로를 설정할 수 없다.

서명된 자격을 native에서 확인하기 전 session은 provisioning_required다. 이후 등록 기록에서 Windows·Android 보호 저장소와 실제 온라인 등록 경계를 연결했으며 학생의 텍스트 수업 화면도 추가했다. 실제 사용자 등록·실기기 실행은 별도 검증으로 남아 있다. 세부 현재 상태는 이 문서 첫머리의 후속 기록을 따른다.

코어는 7일 이내의 서명 자격, 만료 시 읽기/복구, 관측 wall-clock high-water rollback을 처리한다. 등록 후속 구현은 PIN 변경·백업 암호 복구·암호화 백업/복원, 활성 발급키와 과거 검증키의 분리, 새 epoch 서명 시각에 의한 monotonic anchor를 연결했다. 재부팅 뒤 시계 되돌림은 여전히 읽기·복구 모드다. 실제 앱 빌드/복원, 온라인 제출 경로의 epoch fencing, 실제 발급키 교체는 검증하지 않았다. 기존 자료·대기 제출은 만료나 새 앱 판본 때문에 삭제하지 않는다.

현재 저장 schema는 v1 bootstrap과 checksum 검증이다. 미래 schema 변경은 기존 migration 파일을 편집하지 않고 새 migration·snapshot·복원 절차를 추가해야 한다. 32 MiB 제한의 암호화 백업/복원 소스는 연결했지만 실제 복원 검수는 남아 있다. 첨부 quota/orphan 처리 고도화, 준비된 수업 패키지 검증·취소·교사 채점·학부모 피드 모델은 후속 기능이다.

## 재현 명령

~~~powershell
Set-Location 'C:/Users/oswtv/Documents/Codex/thinkgrow-offline-app'
python tools/verify_offline_core.py
python tools/release_bridge.py plan --online-repo 'C:/Users/oswtv/Documents/Codex/thinkgrow-platform-integration' --head 088bdec4031a18fc91e18a3130ca10cf97bde198 --output release-plan.json
~~~

prepare 입력 예시는 release/release-spec.example.json이다. 실제 사용할 timestamp·sequence·artifact 경로로 바꾼 뒤 아래와 같이 실행한다. 개인키는 저장소 밖에서 보관하고 암호는 환경변수로만 전달한다.

~~~powershell
python tools/release_bridge.py prepare --online-repo ONLINE_REPO --online-commit FULL_ONLINE_SHA --offline-repo OFFLINE_REPO --spec release-spec.json --output release-output/release-1 --base-url https://YOUR_HOST/releases/release-1 --private-key PRIVATE_KEY_PATH --password-env APP_RELEASE_PASSWORD
~~~

새 앱은 서명된 catalog로 앱/판본/플랫폼 결속을 검증한다. artifact의 .sig만으로 역할을 판단하지 않는다. 기존 catalog의 sequence를 낮추거나 같은 sequence의 내용을 바꾸는 요청은 거부한다. 네트워크 redirect·인증·부분 다운로드는 향후 native fetch adapter의 검증 대상이며 현재 CLI 검사는 내려받은 로컬 파일을 입력한다.

## 검수 기록

[코어/앱 경계 검사](build-evidence/offline-core-2026-11-03.json), [서명 릴리스 통합 검사](build-evidence/release-bridge-2026-11-03.json), [실제 온라인 기준 commit](build-evidence/online-baseline-2026-11-03.json)을 참고한다. GitHub workflow 파일은 준비만 했고 원격 Actions 실행 결과는 없다. 합성 Git 저장소·임시 키·자료만 사용했다.

공식 근거: [SQLite WAL](https://www.sqlite.org/wal.html), [RustCrypto AES-GCM](https://docs.rs/aes-gcm/0.10.3/aes_gcm/), [Ed25519](https://docs.rs/ed25519-dalek/2.2.0/ed25519_dalek/), [Tauri updater](https://v2.tauri.app/plugin/updater/), [GitHub repository_dispatch](https://docs.github.com/en/actions/reference/workflows-and-actions/events-that-trigger-workflows#repository_dispatch).

최종 자동 검사: 코어 24개 + 서명 릴리스 연결 12개 = 36개 통과. 첨부 journal은 Store open 때 자동 재검사하며, 완료된 파일은 ready로 복구하고 누락/변조는 missing으로 보존한다. 정상 사용자 자료를 자동 삭제하지 않는다.

Windows 교사/학생 Release와 Android 학생/학부모 arm64 Debug를 최종 코어로 재빌드했다. [산출물 해시·APK 검사](build-evidence/core-native-builds-2026-11-03.json)를 참고한다. 실제 앱 화면/기기 실행 검수는 수행하지 않았다.
