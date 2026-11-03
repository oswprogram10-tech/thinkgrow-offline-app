# Phase 1: Native App Foundation

Historical phase-1 baseline: Windows Release shells (teacher/student) and Android arm64 Debug APKs (student/parent) built successfully. APK app IDs, native libraries, generated Kotlin packages and debug signatures were checked. Desktop installer bundles remain disabled in this baseline. Later registration, lessons, PIN-change, and trusted-time source work is recorded in [the current registration log](registration-and-lessons-2026-11-03.md); iOS/iPadOS builds remain unverified.

## 1. App Matrix & Entrypoints

| App | Target Platforms | Temporary App ID | Tauri/Native Entrypoint | UI Entrypoint | Build / Run Commands |
|---|---|---|---|---|---|
| **Teacher (교사)** | Windows (Desktop 전용) | `com.thinkgrow.offline.teacher` | `apps/teacher/src-tauri/src/main.rs`<br>`apps/teacher/src-tauri/src/lib.rs` | `apps/teacher/ui/index.html` | `npm run dev:teacher`<br>`npm run build:teacher` |
| **Student (학생)** | Windows, Android, iPadOS | `com.thinkgrow.offline.student` | `apps/student/src-tauri/src/main.rs` (Windows)<br>`apps/student/src-tauri/src/lib.rs` (Desktop + Mobile) | `apps/student/ui/index.html` | `npm run dev:student`<br>`npm run build:student`<br>`npm run build:student:android`<br>`npm run build:student:ios` |
| **Parent (학부모)** | Android, iOS (Mobile 전용) | `com.thinkgrow.offline.parent` | `apps/parent/src-tauri/src/lib.rs` (Mobile entry point) | `apps/parent/ui/index.html` | `npm run dev:parent`<br>`npm run build:parent:android`<br>`npm run build:parent:ios` |

> [!IMPORTANT]
> - **관리자 앱 배제**: 오프라인 관리자 앱은 없으며, 어떤 오프라인 앱에서도 관리자 로그인을 지원하지 않습니다.
> - **번들 게임 0개**: 기본 번들 게임은 0개이며, 모든 게임은 승인된 별도 자원팩 계약 또는 마켓플레이스를 통해서만 설치·연동됩니다.
> - **오프라인 우선 & 데이터 보존**: 세션 만료 시에도 로컬 데이터(임시저장, 대기열 등)를 삭제하지 않고 읽기/복구 모드로 전환합니다.
> - **학생 데이터 격리**: 학생 앱에는 정답, 교사 메모, 타 학생 기록이 절대 전달되지 않습니다.

---

## 2. Platform & Toolchain Audit (2026-11-03)

| Component | Status | Verified Details | Action Needed |
|---|---|---|---|
| **OS Host** | Available | Windows x64; registry reports Windows 10 Home 25H2, build 26200 | Teacher and student Windows shell builds pass on this host |
| **Disk Space** | Available | C: free space after core implementation/builds: **~13 GiB** | App outputs and role-specific build caches retained |
| **Node.js & npm** | Installed | Node.js `v24.16.0`, npm `11.13.0` | Ready (used for workspace orchestration) |
| **Tauri CLI** | Locked | `@tauri-apps/cli` locked to `2.12.4` in `package-lock.json` | Dependency locked |
| **Visual Studio C++ / Windows SDK** | Installed | Visual Studio Community 2026 x64 C++ tools and Windows SDK `10.0.26100.0`; `kernel32.lib` is present | Teacher and student Windows Release shell builds succeeded; bundle generation is disabled |
| **WebView2 Runtime** | Installed | Microsoft Edge WebView2 version `156.0.4314.58` | Windows Webview runtime ready |
| **Java JDK** | Installed | Eclipse Adoptium OpenJDK `21.0.12.101-hotspot` (`JAVA_HOME` configured) | Android build prerequisite met |
| **Rust / Cargo** | Installed | rustc 1.99.1, Cargo 1.99.1; stable x86_64-pc-windows-msvc; minimal profile | Windows target builds verified for teacher and student shells |
| **Android SDK / ADB** | Installed | Command-line tools, API 37.0, Build-Tools 37.0.0, Platform-Tools, NDK 30.0.16248370; Rust aarch64-linux-android | Student and parent arm64 Debug APK builds and package checks passed; other ABIs/runtime unverified |
| **macOS / Xcode** | **Absent** | Not a macOS host | **Deferred / Waiting** for Mac hardware access (Student iPadOS, Parent iOS) |

---

## 3. Security & App Shell Configuration

- **Strict Offline CSP**:
  모든 앱(`tauri.conf.json`)의 Content Security Policy는 다음과 같이 외부 네트워크 및 원격 리소스를 원천 차단합니다:
  ```text
  default-src 'self'; img-src 'self' asset: http://asset.localhost data:; style-src 'self'; script-src 'self'; connect-src ipc: http://ipc.localhost https://ipc.localhost; object-src 'none'; base-uri 'self'; frame-ancestors 'none'
  ```
- **CDN / 원격 폰트 배제**:
  모든 UI(`ui/index.html`, `ui/app.css`)는 로컬 시스템 폰트(`Segoe UI`, `Malgun Gothic`, `sans-serif`)만을 사용하며 외부 CSS/JS/웹폰트 링크를 일절 포함하지 않습니다.
- **최소 권한 Capability**:
  `capabilities/main.json`은 `["core:default"]`만 선언되어 있으며, 승인되지 않은 네이티브 권한이나 역할 간 권한 혼용이 없습니다.
- **Windows 실행 파일 아이콘**: 교사·학생 역할별 SVG 원본(`apps/teacher/src-tauri/icons/app-icon.svg`, `apps/student/src-tauri/icons/app-icon.svg`)과 Tauri 생성 Windows 리소스 아이콘(`icon.ico`)을 추가했습니다.
- **번들링 & 자동 업데이트 비활성화**:
  모든 앱의 `bundle.active`는 `false`로 설정되어 있으며, 서명 키나 미검증 업데이트 엔드포인트 URL은 공란/비활성화 상태를 유지합니다.

---

## 4. Phase 1 Gates & Status

- [x] Node.js, npm, Java, MSVC C++ Build Tools, WebView2 Runtime 로컬 환경 검사 완료
- [x] Tauri 2.12.4 기준 `package-lock.json` 생성 및 npm workspace 스크립트 정비
- [x] 역할별 앱 분리 및 설정 점검 (교사: Windows 전용 / 학생: Windows+Android+iPadOS / 학부모: Android+iOS)
- [x] 외부 CDN, 원격 폰트, 불필요한 인터넷 연결 전면 배제 및 엄격한 CSP 설정
- [x] 번들링 비활성화 및 서명/업데이트 설정 미검증 URL 제거
- [x] Rust stable MSVC 1.99.1 and Windows SDK 10.0.26100.0 installed; `kernel32.lib` confirmed.
- [x] Teacher and student Windows Release executables built; app bundle generation remains disabled and runtime behavior was not exercised.
- [x] Android SDK / NDK 및 Rust arm64 타깃 설치; 학생·학부모 arm64 Debug APK 빌드 및 패키지·개발 서명 확인 완료 (기기 실행·자동 테스트 미실시).
- [ ] **[대기 - 하드웨어 부재]** macOS/Xcode 및 실기기 확보 후 iPadOS / iOS 타깃 빌드 검증
- [x] Windows Rust SQLite 강제 종료/마이그레이션/백업 복구와 Windows Credential Manager 최소 실험 통과. 제품 앱·모바일 adapter는 미검증.
- [x] Windows native TLS/WebSocket loopback 최소 실험 통과 (30명 가상 접속 포함). 학교 LAN·실기기 검수는 이번 범위에서 제외.
- [ ] 실제 설치 프로그램의 서명 기반 자동 업데이트/롤백은 미검증. Tauri 호환 서명과 시험 전환 모델 12개는 통과.
## 5. Android Build Evidence (2026-11-03)

Student and parent were built through the same role-specific wrapper, including the Gradle callback. `CARGO_TARGET_DIR` is isolated under `target/android/<role>` to prevent package-specific Kotlin output reuse across apps. The shared-cache parent failure (`TauriActivity` missing) was resolved by this separation. Superseded shared Android caches (~5 GiB) were removed after the new JNI symlink targets and APKs were checked.

Both APKs use min SDK 24, target SDK 37, version 0.1.0 (versionCode 1000), and only `arm64-v8a`. They are signed with the local Android Debug certificate; `apksigner verify` passed. Desktop `bundle.active: false` does not prevent the explicit Android `--apk` build. These APKs do not establish production signing or runtime support.

See [Android build procedure](android-build.md) and [package audit JSON](build-evidence/android-2026-11-03.json) for commands, hashes, byte sizes, package IDs and native library names. The Android generated project sources are tracked; build products, generated Kotlin, local paths and keys are excluded by ignore rules. Tauri/Java deprecation warnings remain nonfatal; no warning suppression was added.

## 6. Windows Foundation Probes (2026-11-03)

32 host experiments passed. See [scope and limitations](foundation-validation-2026-11-03.md) and [raw evidence](build-evidence/foundation-2026-11-03.json). These results do not complete the actual installer or mobile runtime gates.

## 7. Native core integration (2026-11-03)

Core storage/transport checks 24 and release bridge checks 12 passed. Windows teacher/student Release and Android student/parent arm64 Debug were rebuilt with the final core. APK app IDs, signatures and native libraries were verified. [Core and update bridge scope](core-and-update-bridge-2026-11-03.md), [native artifact hashes](build-evidence/core-native-builds-2026-11-03.json).

Actual registration/vault/online adapters, student workflow UI and platform updater installation remain unconnected. No installer/production-update completion is inferred from candidate staging or successful compilation.
