# Windows 설치 배포와 기기 등록 — 2026-11-03

> 이 문서는 첫 Windows 배치의 역사적 기록이다. 실제 등록 서버·모바일 vault·수업 준비의 최신 상태는 [후속 진행 기록](registration-and-lessons-2026-11-03.md)을 따른다. 아래의 미배포·공개키 비어 있음 표기는 첫 배치 당시 상태다.

## 사용자 결정

Windows 기본 배포물은 교사·학생 각각의 Setup.exe이며 무설치 ZIP을 보조 제공한다. 다운로드 화면은 먼저 공개하되 실제 파일은 전체 목표 기능과 정식 배포 준비가 끝난 뒤 공개한다. 설치판의 교사·학생·학부모는 독립 앱이며 관리자 설치판은 없다. 사용자는 이번 변경의 필요한 자동 검사 실행도 승인했다.

## 현재 구현

- tauri.windows.conf.json에서 교사·학생별 NSIS 설치를 활성화했다. 한국어, 현재 Windows 사용자 설치, 역할별 app ID·설치 위치·바로가기, 구버전 덮어쓰기 차단을 사용한다. Android·Apple 설정에는 Windows 번들을 적용하지 않는다.
- 첫 설치가 인터넷에 의존하지 않도록 WebView2 오프라인 설치 프로그램을 포함한다. 제작 시의 런타임·NSIS 도구 다운로드와 수업 중 실행 의존성은 구분한다. 무설치 ZIP은 WebView2가 이미 설치된 PC에서 실행하는 보조 배포물이며, 학습 자료가 USB와 함께 이동하는 형태는 아니다.
- 공식 Tauri CLI 2.12.4 NSIS 템플릿을 고정해 자료 삭제 체크박스와 앱 데이터 삭제 분기만 제거했다. installer/windows/UPSTREAM.json과 MIT 고지를 보존한다. 제거·업데이트가 계정 키와 학습 자료를 자동 삭제하지 않게 구성했다. 실제 OS 설치·제거 전 과정 검증을 완료했다는 뜻은 아니다.
- tools/package_windows.py는 새 출력 폴더에만 설치 프로그램·ZIP·SHA-256·개발 빌드 명세를 만든다. 기존 폴더를 덮어쓰지 않으며 업로드 기능이 없다. 앱 코드서명과 정식 업데이트 서명은 아직 적용하지 않았다.
- 계정별 데이터 키와 기기 서명키를 Windows 사용자 DPAPI로 보호한다. 추가로 Argon2id에서 유도한 숫자코드 키로 내부 자료를 암호화한다. 숫자코드는 6~12자리이며 5회 오류 뒤 대기 시간을 기록한다. 키·숫자코드를 WebView 응답이나 서버에 보내지 않는다. 평문 저장으로 돌아가는 우회 경로는 없다.
- 앱을 다시 열면 잠긴 상태에서 시작한다. 잠금 명령·화면 숨김·5분 비활성 잠금과 비밀번호 입력 지우기를 연결했다. 계정별 명령을 각 앱 capability에 등록했으며 답안 저장·제출은 학생 앱에만 있다.
- 등록 요청 전에 기기키와 요청 ID를 보관한다. 연결 실패 뒤 동일한 보관 계정을 선택하면 같은 기기키·요청 ID로 재요청한다. 발급 자격은 고정 공개키·앱 ID·기기 ID·공개키·기간·scope를 검증한 뒤에만 연결한다.
- 만료 자격으로 다시 열어도 초안과 대기 자료를 보존하고 읽기·복구 상태를 유지한다. 실행 중 monotonic anchor와 관측 wall-clock high-water를 사용한다. 재부팅 간 완전한 신뢰 시간·키 회전·분실 숫자코드 복구는 별도 구현 항목이며 완료로 표시하지 않는다.
- 앱 UI는 기존 밝은 배경·녹색 행동 버튼을 주 참조로 고정했다. Refero 실시간 도구 미연결 상태에서 번들 craft 지침의 명시적 입력 라벨·44px 조작 영역·초점 표시·인라인 오류를 적용했다. 2열 PC·1열 좁은 화면을 사용하며 그림과 신규 UI 프레임워크를 추가하지 않았다.

## 서버 연결 상태

온라인 저장소에 /api/offline/v1/devices/code와 /redeem을 구현했다. 코드 발급은 기존 로그인과 역할·활성 계정·배정 권한을 확인하며, 코드 원문은 DB에 저장하지 않는다. 코드는 10분, 발급 자격은 최대 7일이다. redeem은 원자적 장부에서 요청·기기·공개키를 결속하고 동일 요청에 동일 서명 자격을 반환한다. 역할 변경·다른 기기 재사용·철회·구 epoch 재생은 거부한다. 학생 권한은 기존 과제 권한 검사에서 추출하며 정답·교사 메모를 보내지 않는다.

서버 소스는 작성·로컬 검증만 했으며 운영 배포하지 않았다. 실제 발급키·native 공개키를 아직 설정하지 않았다. release/native-policy.json의 빈 공개키 목록을 합성 검사 키로 대체하지 않는다. 현재 제품 화면은 등록 서비스를 준비 중으로 표시하고 새 등록 버튼을 비활성화한다. Android Keystore·Apple Keychain 어댑터도 아직 연결하지 않았으므로 해당 플랫폼을 사용 가능하다고 표시하지 않는다.

## 검사와 빌드 근거

- 코어 자동 검사 32개(새 Windows 계정 보호 8개 + 기존 저장·전송 24개) 통과.
- 서명 릴리스 연결 검사 12개 통과.
- 합성 응답을 사용하는 브라우저 화면 검사 6개 통과. 실제 native UI·운영 서버 검증과 구분한다.
- 온라인 기기 등록 검사 11개(실제 로컬 HTTP 라우터 포함)와 기존 기반 회귀 15개 통과. 실제 Firebase 서버에 쓰지 않았다.
- 교사·학생·학부모 native 명령 Windows 컴파일 통과. 부모 앱 Windows 배포물을 추가한 것은 아니다.
- Windows EXE·설치 프로그램·ZIP은 개발용 미공개 산출물이다. Android/Apple의 이번 변경 타깃 빌드·실행 검증은 별도로 남아 있다.

## 남은 순서

| 순서 | 남은 일 | 완료 조건 |
|---|---|---|
| 1 | 운영 기기 발급키·native 공개키·등록 코드 발급 화면, Android/Apple 보호 저장소 | 실제 역할별 등록·잠금·갱신, 키 회전·복구 경로 연결 |
| 2 | 서명 수업 패키지와 학생 단독 수업 화면 | 권한 내 수업만 준비, 정답 제외, 작성·재시작 복원 |
| 3 | 교실·온라인 전송 수신기 | 중복 수신 장부·receipt·교사 relay·취소·충돌·지연 제출 처리 |
| 4 | 교사 채점·학부모 피드·게임 설치/실행 연결 | 역할별 공개 범위와 기록 보존을 포함한 실제 흐름 |
| 5 | 앱 서명·HTTPS 업데이트 피드·플랫폼 설치/복구·정식 배포 | 검증한 릴리스만 수업 종료·동의 후 적용, 자료 보존 |

GitHub 원격·운영 발급키·운영 API 배포·Apple 빌드 환경이 없는 상태를 완료로 처리하지 않는다. 전체 설치판 구현 완료를 선언하는 문서가 아니다.

## 재현

~~~powershell
node scripts/build-ui.mjs teacher
node scripts/build-ui.mjs student
node scripts/build-ui.mjs parent
cargo test --locked -p thinkgrow-offline-core
cargo check --locked -p teacher -p student -p parent
node tools/verify_account_ui.mjs
python tools/test_release_bridge.py
python tools/package_windows.py --roles teacher student --output release-output/NEW-DIRECTORY
~~~

근거: https://v2.tauri.app/distribute/windows-installer/ , https://github.com/tauri-apps/tauri/issues/15983 , https://learn.microsoft.com/en-us/windows/win32/api/dpapi/nf-dpapi-cryptprotectdata , https://docs.rs/argon2/0.5.3/argon2/ . Tauri MIT 템플릿은 기존 설치·업데이트 경로를 재사용하고 자료 삭제 두 부분만 수정했다. keyring-rs와 비교했으나 Windows 우선 구현에는 DPAPI를 직접 사용하여 앱/현재 사용자 결속과 파일 원자 교체를 명시했다. 새 주요 의존성은 Argon2 0.5.3, reqwest 0.13.5(native Windows TLS), windows-sys 0.61.2다.

최종 개발 패키지: release-output/windows-development-20261103-r2/build-manifest.json. 4개 파일 합계 450,620,452바이트이며 EXE/ZIP 해시와 ZIP 내 x64 PE·고지 파일을 대조했다. 이전 개발 출력과 해시가 같은 중복 번들만 제거해 892,680,154바이트를 정리했다. [현재 검사·빌드 근거](build-evidence/windows-registration-2026-11-03.json)를 참고한다. 온라인 저장소 Graphify AST 갱신도 완료했다(34,369 nodes / 75,168 edges).

추가 배포 소스 검사 4개도 통과했다. 신규 API 두 모듈은 SOURCE_FILES와 Docker 허용 목록에 모두 포함한다. 서로 다른 자동 검사 항목은 총 80개 통과했다(반복 실행은 중복 집계하지 않음).

공통 코어는 Android arm64(aarch64-linux-android) cargo check도 통과했다. Windows 전용 API 의존성이 Android에 강제로 연결되지 않음을 확인했다. 이번 검사로 APK 재빌드·Android 보호 저장소·기기 실행이 완료된 것으로 해석하지 않는다.
