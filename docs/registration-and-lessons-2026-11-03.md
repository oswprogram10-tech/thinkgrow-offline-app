# 기기 등록과 학생 수업 준비 진행 기록 — 2026-11-03

## 실제 연결한 범위

온라인 등록 페이지 https://schoolclassscorecheck.web.app/offline-register.html 과 전용 Cloud Run thinkgrow-offline-api를 연결했다. 기존 think-grow-api의 00058-976 리비전·설정·트래픽을 보존했다. Firebase Hosting은 기존 파일/설정에서 등록 화면 3개와 /api/offline/** 경로만 추가·갱신한다. 다운로드 catalog는 publication_approved=false, releases=[]이며 설치 파일은 공개하지 않았다.

기기 발급용 새 Ed25519 키는 게임/앱 업데이트 서명키와 분리했다. Windows 현재 사용자 DPAPI 보관 파일은 저장소 밖 %LOCALAPPDATA%/ThinkGrow/OfflineIssuer/offline-credential-20261103.dpapi에 있다. 서버는 전용 Secret Manager 버전 참조만 사용한다. 키 원문을 소스·로그·문서·명령 인수로 쓰지 않는다. 앱에는 공개키만 고정하며 실제 서버 공개키와 대조했다. 이 파일을 삭제하면 후속 릴리스 운영이 어려우므로 정리 대상이 아니다.

공개키 ID: offline-credential-20261103. SHA-256 fingerprint: ee1f8243e341ae5ef183dd4ca82664dba5c848c7d9ada5bced8bf5fe08f1e97c.

설치판은 `credential_keys`에 저장 자료 검증용 공개키를, `credential_signing_key_ids`에 새 자격 발급을 허용할 ID를 분리한다. 더 높은 epoch의 활성 서명 자격은 발급 시각으로 저장소 high-water를 재고정하며 현재 실행 동안 monotonic 경과 시간을 따른다. 재부팅 뒤 과거 시각은 읽기·복구 모드로 보존하고, 캐시 자격은 신뢰 시각을 재설정하지 않는다. 현재 실제 활성 키는 기존 ID 하나이며 키 교체를 실행하거나 검증하지 않았다. 모든 역할의 PIN 변경·백업 암호 복구·32 MiB 암호화 백업/복원 소스를 연결했다. Windows 검증에서 코어 47개, 역할 UI 빌드·계정 화면 6개, 릴리스 브리지 12개가 통과했고 합성 타기기 복원 뒤 epoch 2 갱신도 확인했다. 실기기 검수는 남아 있다. 상세 경계는 [자격 신뢰·복구 설계](credential-trust-and-recovery-2026-11-03.md)를 따른다.

온라인 기존 역할 로그인 후 10분짜리 등록 코드를 발급하고, 앱은 고정 HTTPS 주소에서 기기 키·요청 ID에 결속한 7일 자격으로 교환한다. 발급코드와 기기 PIN은 다르다. 학생은 등록 시 최대 16개 열린 수업을 선택할 수 있고, 기존 계정을 선택해 자격을 갱신하면 자료·기기 키를 보존한다. 실제 사용자 계정으로 발급·등록한 검수는 수행하지 않았다. 공개 서비스의 비로그인/외부 Origin/잘못된 코드 차단과 공개키는 실제 확인했고 나머지 동작은 합성 DB/서명/HTTP로 검증했다.

## 보호 저장소와 타깃별 근거

- Windows: 기존 DPAPI 바깥 보호 + Argon2id/PIN 안쪽 보호 및 계정별 SQLite 암호화를 유지했다. 교사/학생 Setup.exe와 ZIP은 개발용이며 Authenticode/업데이트 정식 서명은 아직 없다.
- Android: 별도 native-vault 플러그인에서 AndroidKeyStore AES-256-GCM을 사용한다. 앱 ID AAD, 임의 nonce, 기존 키 부재 시 복호화 실패 보존을 적용했다. 학생/학부모 arm64 Debug APK와 JVM 암호화 검사 3개를 통과했다. 기기 Keystore 실행·설치 검수를 했다는 뜻이 아니다. 사용자 결정에 따라 학교 LAN/실기기 검수를 다시 필수 항목으로 추가하지 않는다.
- iOS/iPadOS: Keychain WhenUnlockedThisDeviceOnly + CryptoKit AES.GCM 및 고정 URLSession 전송 소스를 구현했다. 이 Windows 환경에서는 Mac/Xcode 타깃 빌드를 수행하지 못했으므로 소스 구현으로만 기록한다.
- Android 자동 백업/기기 이전에서 vault·DB를 제외했다. 앱 제거·OS 초기화·기기 분실까지 무손실이라고 약속하지 않는다. 계정 만료/앱 잠금이 자료 삭제를 실행하지는 않는다.
- Windows 심볼릭 링크 권한 때문에 일반 Tauri Android 포장이 실패한 뒤, 이미 빌드한 정확한 역할의 .so를 검증된 하드링크로 연결해 Gradle 포장을 끝냈다. 개발자 모드나 UAC 정책을 변경하지 않았다. 도구는 경로·samefile·해시·APK 내 역할별 라이브러리를 검증한다.

## 학생 수업 준비와 작성

기기의 서명된 자격 + 현재 시각·UUID·작업·수업 목록에 대한 기기 서명을 서버에서 검증한다. 현재 기기 epoch/철회·계정·학교/반 배정·과제 공개/시작/마감도 다시 검사한다. 응답은 thinkgrow.lesson.v1 + NUL 도메인의 Ed25519 서명이다. 서버는 개인정보나 정답을 담은 snapshot을 통째로 직렬화하지 않는다.

본문·공개 힌트·학생용 기준/승인된 학습 도움과 아래 7유형의 허용 필드만 내보낸다. 정답·채점 해설·교사 메모·다른 학생 자료는 제외한다. 연결형 ID는 기존 비밀키 기반 변환을 사용하고 순서형 ID도 계정/과제에 결속한 HMAC ID로 바꾼다. 공개 내용만 revision hash에 포함해 비공개 정답의 추측 검증에 사용되지 않게 했다. 서술형 공식 글자 수 제한도 보존한다.

| 유형 | 저장하는 학생 응답 |
|---|---|
| 서술형 | body |
| 참·거짓 | selected |
| 객관식 | selected_option_id |
| 연결 | pairs(left_id,right_id) |
| 순서 | ordered_ids |
| 짧은 답 | text |
| 빈칸 | blanks(id:text) |

수업은 기존 핵심 DB와 별도 lessons.sqlite3(schema1)에 계정/기기 바인딩·암호화·서명 검증 후 원자 저장한다. 기존 core schema1/checksum/outbox는 유지한다. 새 수업 판본을 나란히 남기고 작성한 답안은 원래 판본으로 연다. 자격/수업 만료 후에도 읽고 복구할 자료를 지우지 않는다.

학생 native 명령에만 목록/온라인 준비/판본 검증 저장/고정 제출 대기열을 연결했다. 7문항 입력, 자동저장, 화면 이동 전 flush, 잠금 전 flush, 재시작 복원, 실패 시 현 입력 유지, 읽기 전용 표시를 구현했다. WebView에는 키·임의 네트워크 URL·임의 파일 경로를 주지 않는다. 교사/학부모에 학생 명령을 추가하지 않았다. 제출 수신 API는 아직 없으므로 화면은 제출 완료 대신 대기열 보관으로 표시한다.

**현재는 텍스트 수업 범위다.** 필수 첨부·게임·미디어가 있는 수업은 OFFLINE_REQUIRED_CONTENT_UNSUPPORTED로 거부한다. 파일을 빼고 준비 완료로 표시하지 않는다. 암호화 첨부의 코어 기반은 있지만 실제 서버 수업 첨부 전송·미디어 표시·필수 게임 의존성 연결은 남아 있다. 저장중 OS 강제 종료·용량 부족·실기기 실행 보장을 주장하지 않는다.

## 검사와 증거

코어 43개(기존 32 + 수업 10 + Python→Rust 서명 계약 1), 온라인 등록/수업/전용 서버/배포 경계 31개, 학생 브라우저 15개, 계정 브라우저 6개, 등록 브라우저 9개가 통과했다. 합성 검사와 공개 서버 차단 검사·타깃 컴파일은 별개다. iOS 실행이나 실제 사용자 등록 성공으로 합산하지 않는다.

주요 도구: tools/credential_issuer.py, tools/verify_student_ui.mjs, tools/verify_account_ui.mjs, tools/package_windows.py, tools/package_android_linked.py. 온라인 도구: scripts/offline_device_rollout.py(기본 조회 계획, --apply 최초 적용, --update --apply 자기 전용 서비스 갱신). 등록 전용 서버만 다루고 운영 main API 배포 도구를 대신 실행하지 않는다.

## 남은 순서

| 순서 | 다음 구현 | 상태 |
|---|---|---|
| 1 보완 | 백업/복원·PIN 복구·타기기 재결속 실제 검수, 발급키 교체 검증 | Windows 합성 검증과 역할 컴파일 통과. 실제 기기 복원과 운영 발급키 교체 검증은 남음 |
| 2 보완 | 필수 첨부/미디어·수업 게임 의존성 준비 | 텍스트 7유형은 연결, 필수 자산 수업은 안전하게 차단 |
| 3 | 실제 교실/온라인 수신 장부·receipt·relay·취소·충돌·지연 검토 | 코어 기반만 있으며 실제 수신 어댑터 미연결 |
| 4 | 교사 채점·학부모 소식·게임 설치/실행 | [후속 연결](role-domains-and-games-2026-11-03.md): 채점 초안/온라인 반영·공개 보고서/메시지/공지·회신·승인 정적 게임 설치/배정/실행 연결. 서버/교실 게임과 진행 전달은 후속 |
| 5 | 정식 앱 서명·HTTPS 업데이트/플랫폼 적용·복구·GitHub 자동화 | 실제 설치/업데이트 구현 필요. 설치판 원격은 https://github.com/oswprogram10-tech/thinkgrow-offline-app |
| 별도 타깃 | Apple 소스의 Mac/Xcode 빌드 | 환경이 없어 미검증 |

전체 설치판/정식 릴리스 완료를 의미하지 않는다. 앞선 Windows 문서와 build-evidence는 각 작성 시점의 기록이며 최신 연결 상태는 이 문서를 우선한다.

## 최종 개발 산출물

Windows 교사/학생 r3의 설치 프로그램 2개·ZIP 2개 합계 451,227,866바이트를 만들고 해시·ZIP 안의 x64 PE·역할별 명령 경계를 확인했다. release-output/windows-development-20261103-r3/build-manifest.json과 docs/build-evidence/registration-and-lessons-2026-11-03.json을 따른다. Android는 docs/build-evidence/mobile-vault-student-2026-11-03.json 및 mobile-vault-parent-2026-11-03.json에 최신 APK/.so 해시를 기록했다. 모두 미공개 개발 빌드다.

새 출력 검증 뒤 대체된 r2 개발 출력과 target의 동일 설치 프로그램 사본만 정리해 892,930,311바이트를 회수했다. 사용자 기록·키·현재 r3는 보존했다. 정리 근거는 docs/build-evidence/development-output-retirement-2026-11-03.json이다.
