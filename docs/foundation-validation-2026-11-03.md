# Windows 기반 최소 실험 — 2026-11-03 KST

**이 PC의 32개 실험 통과.** [원본 결과](build-evidence/foundation-2026-11-03.json)의 UTC 시각은 KST 2026-11-03에 해당한다. 제품 앱에 저장·인증·동기화·업데이터를 연결한 상태는 아니다.

| 영역 | 사례 | 확인 결과 |
|---|---:|---|
| SQLite | 8 | Rust rusqlite 0.41.1 / bundled SQLite 3.54.1, WAL + synchronous=FULL. 커밋 전·후 각각 3회 TerminateProcess, 초안·대기 작업·무결성 보존, 실패한 migration rollback, 손상 감지와 native backup API 복원 |
| 자격 증명 | 3 | Rust keyring의 Windows Credential Manager backend. 프로세스 간 재조회, 역할별 namespace, 회전·삭제 |
| TLS | 4 | Rust native-tls / Windows SChannel. 전용 CA + localhost 정상 접속, 다른 CA·다른 호스트명·만료 인증서 거부 |
| WebSocket | 5 | 시험 토큰·역할 거부, 수신 commit 후 응답 유실·재접속 시 같은 receipt, 같은 ID의 다른 payload 거부, 30명 가상 클라이언트 |
| 업데이트 모델 | 12 | Tauri CLI 2.12.4 임시 서명과 Rust minisign 검증 호환, 변조·다른 키 거부, 역할·버전·수업 중·동의 정책, 전환 전·후 강제 종료와 health 실패 복구, 정상 전환 시 사용자 DB 불변 |

## 결과의 경계

- SQLite는 실제 Rust 네이티브 DB를 사용했다. 프로세스 강제 종료를 다뤘으며 전원 차단·디스크 고장/용량 부족·DB/첨부 암호화·제품 schema migration은 별개다. WAL DB는 로컬 디스크에 두며 네트워크 공유 드라이브에 두지 않는다.
- Windows vault의 역할별 서비스명은 충돌 방지다. 같은 Windows 사용자로 실행되는 다른 프로세스에 대한 앱별 접근 제어를 증명하지 않는다. 앱의 제한된 native API에서 역할·기기·계정 범위를 검증해야 한다. Android Keystore·Apple Keychain은 미검증이다.
- 통신은 127.0.0.1 임시 포트에만 바인딩했다. TLS 검증을 끄거나 OS 루트·방화벽·개발자 모드를 바꾸지 않았다. 서버는 Python ssl/WebSocket 시험 수신기다. 시험 토큰은 실제 기기 자격·서명 proof가 아니다. 30명은 이 PC의 가상 클라이언트이며 학교망/30대 기기의 성능 증거가 아니다.
- 업데이트는 실제 서명 검증과 시험용 A/B 전환 저널 모델이다. 후보는 JSON fixture다. EXE/MSI/NSIS/APK 교체·실행, 파일 잠금, 설치 권한, 실제 Tauri updater plugin, 정식 코드 서명, 모바일 스토어 업데이트, schema downgrade, 전원 장애 원자성은 검증하지 않았다. 이 모델을 제품 업데이터로 사용하지 않는다.
- 서명된 fixture는 앱 ID·버전·payload를 함께 담는다. 실제 installer에서는 앱/채널/버전/플랫폼 결속과 signed-version 정책을 연결해야 한다. CLI의 시험 서명 생성은 제품 배포 서명키 생성이 아니다.
- 학교 LAN·실기기 검수는 이번 범위에서 제외했다. 운영 서버·실제 학생 자료·기존 게임 배포키를 사용하지 않았다. Android/iOS native TLS와 WebView/native 브리지도 이 결과로 판정하지 않는다.

## 재현

일반 Windows 사용자 세션에서 실행한다. Rust/MSVC/Windows SDK와 Node/npm은 앞 단계의 설치 상태를 사용한다. 검증 crate는 제품 Tauri workspace와 분리했으며 앱 의존성·WebView 권한을 추가하지 않았다.

~~~powershell
Set-Location 'C:\Users\oswtv\Documents\Codex\thinkgrow-offline-app'
$env:Path = "$env:USERPROFILE\.cargo\bin;" + $env:Path
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target\foundation-probe'
cargo build --locked --manifest-path tools\foundation-probe\Cargo.toml
python -m venv tools\foundation-probe\.venv
tools\foundation-probe\.venv\Scripts\python.exe -m pip install -r tools\foundation-probe\requirements-lock.txt
tools\foundation-probe\.venv\Scripts\python.exe tools\foundation-probe\run.py
~~~

설치·빌드에는 다운로드가 필요할 수 있다. 준비된 실행기의 검증 트래픽은 loopback뿐이다. 재실행 시 JSON은 최신 실행 결과로 바뀐다.

시험 DB·CA·개인키는 매번 target/foundation-runs 아래 임의 ID 디렉터리에 만들고 종료 시 삭제한다. 자격 증명도 com.thinkgrow.offline.역할.foundation-probe.임의ID 이름으로만 만들고 삭제·조회 실패까지 확인한다. 비밀 원문은 JSON에 넣지 않는다. 정상 종료 뒤 시험 디렉터리가 비어 있는 것을 확인했다.

## 다음 구현에 반영할 결정

1. 로컬 SQLite transaction과 WAL/FULL로 초안·고정 제출본·outbox를 원자 저장한다. 자격 만료와 자료 보존/암호화키 수명을 분리한다. 활성 WAL을 단순 복사하지 않고 SQLite backup API로 백업한다.
2. 역할·계정별 native 경계를 만들고 자격 증명 원문을 WebView에 전달하지 않는다. Windows와 모바일 adapter를 같은 저장 계약 아래 별도로 구현·검증한다.
3. 수신 commit 뒤 receipt를 보내고 응답 유실 시 원래 operation ID로 재전송한다. 시험 수신기를 운영 인증/전송 코어로 간주하지 않는다.
4. 업데이트는 수업 종료 후 동의, 서명·역할·판본 검사, 사용자 자료 영역 분리, 후보 health 확인 후 확정으로 구현한다. 실제 installer 통합 전 자동 업데이트 gate는 열린 상태로 둔다.

## 근거

[SQLite WAL](https://www.sqlite.org/wal.html), [backup API](https://www.sqlite.org/backup.html), [rusqlite](https://docs.rs/rusqlite/0.41.1/rusqlite/), [Windows CredWrite](https://learn.microsoft.com/en-us/windows/win32/api/wincred/nf-wincred-credwritew), [keyring](https://docs.rs/keyring/3.6.3/keyring/), [Tauri updater](https://v2.tauri.app/plugin/updater/), [minisign verifier](https://docs.rs/minisign-verify/0.2.4/minisign_verify/).