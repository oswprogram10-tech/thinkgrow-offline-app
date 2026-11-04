# FIX-OFF-01 수업 자료 평문 노출 줄이기와 내려받기 검증 강화

## 목표
1. 복호화한 자료 임시 파일이 비정상 종료 후에도 남는 문제를 줄인다.
2. 내려받는 중인 자료 조각이 평문으로 SQLite에 저장되지 않게 한다.
3. WebView가 넘긴 자료 정보(`MaterialInfo`)와 조각을 네이티브 쪽에서 검증한다.

## 현재 상태 (`crates/offline-core/src/`)
- `class_materials.rs:351-358` `export_ephemeral_material`: `%TEMP%\tg-mat-<uuid>\파일명`에 평문 저장. 정리는 `runtime.rs:114-125`의 잠금·Drop 때만.
- `class_materials.rs:179-196` `save_material_chunk`: `download_chunks.chunk_data`에 평문 저장, 다 모인 뒤에만 `self.seal`.
- 같은 함수는 `chunk`와 `info`가 서로 맞는지, 학생이 받을 권한이 있는 자료인지 확인하지 않는다. 실행 경로(`workspace.rs:196-203` `start_material_run`)는 학생에 대해 서명된 작업공간과 대조한다.
- 봉인·복호화 함수: `store.rs:252` `seal(aad, plain)`, `store.rs:253` `decrypt(aad, cipher)`.

## 구현 결정
1. **임시 파일 이름과 시작 시 정리**
   - 임시 폴더 이름을 `tg-mat-<앱>-<uuid>`로 바꾼다. `<앱>`은 `teacher`/`student`/`parent`(Store의 `app` 값에서). 교사·학생 앱이 같은 PC에 있어도 서로의 파일을 지우지 않게 하려는 것이다.
   - `class_materials.rs`에 `pub fn cleanup_stale_ephemeral(app: App)`을 추가한다. `std::env::temp_dir()` 바로 아래에서 이름이 `tg-mat-<앱>-`으로 시작하는 **디렉터리만** 지운다(`fs::remove_dir_all`). 심볼릭 링크는 따라가지 않고 건너뛴다(`symlink_metadata`로 확인). 실패는 무시한다(다른 프로그램이 파일을 열고 있을 수 있음).
   - `runtime.rs` `NativeContext::new`에서 한 번 호출한다.
2. **조각 봉인**
   - `save_material_chunk`에서 조각을 `self.seal(&format!("materials-chunk/{}/{}", material_id, offset), &chunk_bytes)`로 봉인한 값을 `chunk_data`에 넣는다.
   - 조립할 때 각 행을 같은 aad로 `self.decrypt`한 뒤 이어 붙인다. 복호화 실패(예: 이전 버전이 남긴 평문 조각)면 그 자료의 `download_chunks` 행을 모두 지우고 `Error::Invalid("stale download chunks")`를 반환해 다시 내려받게 한다.
   - DB 스키마(열 이름·형식)는 바꾸지 않는다.
3. **입력 검증** (`save_material_chunk` 맨 앞)
   - `chunk.material_id == info.id`, `chunk.sha256 == info.sha256`, `chunk.total_size == info.size`
   - `0 < info.size <= MAX_MATERIAL_BYTES`. 상수 `MAX_MATERIAL_BYTES: u64 = 512 * 1024 * 1024`을 추가한다(서버 최대인 동영상 500MB 기준).
   - `chunk.offset + chunk.length <= chunk.total_size`(오버플로는 `checked_add`)
   - 하나라도 어긋나면 `Error::Invalid`.
   - **학생 앱만**: `start_material_run`의 학생 대조 조건(작업공간 epoch·만료, 과제 범위, `materials` 배열에 `id`·`assignment_id`·`source_sha256` 일치)을 `workspace.rs`에 `pub(crate) fn authorize_student_material(&self, info: &MaterialInfo, now: i64) -> Result<()>`로 뽑아내고, `start_material_run`과 `save_material_chunk` 둘 다 이 함수를 쓴다. `save_material_chunk`에 `now: i64` 인자를 추가하고, `runtime.rs:95`의 호출부는 `start_material_run` 호출부가 `now`를 얻는 방식 그대로 맞춘다.
   - 교사 앱은 일관성 검사만 한다(실행 경로도 교사는 작업공간 대조가 없으므로 동작을 맞춘다).

## 건드리지 말 것
- 역할별 명령 목록·capability 파일(`apps/*/src-tauri/capabilities`, `permissions`)
- 서명 도메인, 계약 파일(`contracts/`), 등록·자격 로직
- 자료를 OS 기본 앱으로 여는 동작 자체(평문 임시 파일은 그 때문에 필요하다)

## 테스트 (`crates/offline-core/tests/core.rs`에 추가)
- `material_chunks_are_sealed_at_rest`: 조각 1개 저장 후 `materials.sqlite3`의 `chunk_data`에 원문 바이트가 포함되지 않음
- `material_chunk_mismatch_is_rejected`: id·sha256·total_size 불일치, 크기 0, 상한 초과, offset+length 초과 각각 거부
- `student_cannot_store_unassigned_material`: 작업공간에 없는 자료 → `Error::Authorization`
- `stale_plaintext_chunks_force_redownload`: 평문 행을 직접 넣은 뒤 마지막 조각 저장 → 오류, 해당 행 삭제됨
- `cleanup_stale_ephemeral_only_removes_own_app_dirs`: 교사 정리 후 `tg-mat-student-*`는 남음(테스트는 임시 `TMP`/`TEMP`를 별도 폴더로 지정해 실제 임시 폴더를 건드리지 않게)

## 완료 확인
- [ ] `cargo test --locked -p thinkgrow-offline-core` 통과
- [ ] `cargo check -p teacher -p student -p parent` 통과
- [ ] 위 테스트 5개 존재·통과
- [ ] 커밋 1개
