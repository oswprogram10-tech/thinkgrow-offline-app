# Android 셸 빌드

학생과 학부모는 별도 Android 프로젝트와 앱 ID를 사용한다. 첫 검증 대상은 `arm64-v8a` Debug APK다. 다른 ABI, 기기 실행, 오프라인 기능, 정식 배포 서명은 이 빌드 결과에 포함하지 않는다.

## 도구체인 (2026-11-03)

- JDK 21.0.12, Rust/Cargo 1.99.1, Rust target `aarch64-linux-android`
- Google command-line tools archive `commandlinetools-win-15859902_latest.zip` (공식 SHA-256 검증 완료)
- Android SDK Platform `platforms;android-37.0`, Build-Tools `37.0.0`, Platform-Tools
- Android NDK `30.0.16248370`
- SDK: `%LOCALAPPDATA%\Android\Sdk`; 사용자 환경변수 `ANDROID_HOME`, `NDK_HOME` 설정
- Tauri CLI `2.12.4`; 생성된 Gradle wrapper `9.6.1`, Android Gradle Plugin `9.3.1`

Android Studio와 에뮬레이터는 설치하지 않았다. SDK 설치·의존성 다운로드에는 인터넷이 필요하며, 이는 수업 중 앱 실행 의존성을 뜻하지 않는다.

## Windows 빌드 명령

Tauri Android 빌드는 네이티브 라이브러리에 심볼릭 링크를 만든다. 이 PC에서는 일반 사용자 빌드가 링크 생성 권한에서 실패했다. **관리자 권한 PowerShell**에서 다음 명령을 실행한다. 개발자 모드 등 시스템 정책은 변경하지 않았다.

```powershell
Set-Location 'C:\Users\oswtv\Documents\Codex\thinkgrow-offline-app'
npm.cmd run build:student:android -- --debug --target aarch64 --apk --ci
npm.cmd run build:parent:android -- --debug --target aarch64 --apk --ci
```

새 터미널에 사용자 환경변수가 반영되지 않았다면 `ANDROID_HOME`과 `NDK_HOME`을 사용자 환경변수에서 현재 세션으로 읽는다.

```powershell
$env:ANDROID_HOME = [Environment]::GetEnvironmentVariable('ANDROID_HOME', 'User')
$env:NDK_HOME = [Environment]::GetEnvironmentVariable('NDK_HOME', 'User')
```

## 역할별 생성물 분리

`scripts/tauri-mobile.mjs`는 Android 명령의 `CARGO_TARGET_DIR`을 역할별로 고정한다.

| 앱 | 앱 ID | Cargo 생성물 경로 |
|---|---|---|
| 학생 | `com.thinkgrow.offline.student` | `target/android/student` |
| 학부모 | `com.thinkgrow.offline.parent` | `target/android/parent` |

공용 Cargo target을 사용할 때 Tauri 의존성의 Kotlin 생성 결과가 학생 앱에서 학부모 앱으로 재사용되어 `TauriActivity`가 누락됐다. 앱의 Android 진입 명령과 Gradle의 `npm run tauri` 콜백이 모두 같은 역할별 래퍼를 사용하도록 구성했다. 학생의 생성 코드를 학부모로 복사하지 않는다.

각 앱의 `src-tauri/gen/android` 소스는 Git 추적 대상이다. Gradle 캐시, 로컬 SDK 경로, 생성된 Kotlin 코드, 네이티브 `.so`, APK와 빌드 산출물은 하위 `.gitignore`로 제외한다. 역할별 SVG 원본에서 PNG와 Android 런처 리소스를 생성했다.

## 범위

Debug APK는 개발용 서명을 사용한다. `bundle.active: false`는 Windows 설치 패키지 번들을 비활성화하는 설정이며, 명시적인 Android `--apk` 빌드에는 APK가 생성된다. 앱 실행과 기기 검수, 자동 테스트는 수행하지 않았다. 저장·인증·게임 실행·동기화는 아직 최소 셸에 구현되지 않았다.

참고: [Tauri 필수 도구](https://v2.tauri.app/start/prerequisites/), [Google SDK 도구](https://developer.android.com/studio#command-line-tools-only).