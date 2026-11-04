# OFF-SUB-01 학생 제출 온라인 직접 전송

설계: `docs/submission-delivery-design-2026-11-04.md`. 짝 카드: 플랫폼 저장소 `.mece/cells/PLT-SUB-01/TASK.md`(서버 수신 API, 별도 진행).

## 목표
학생 앱의 제출 대기열을 인터넷이 될 때 서버로 보내고, 서명된 online receipt를 받으면 "제출 완료"로 표시한다. 마감 판정을 위해 제출 시각을 기기 서명에 포함한다.

## 현재 상태
- `crates/offline-core/src/contract.rs`: `Operation`(68줄), `Envelope::verify`(77줄, `o.protocol!=PROTOCOL` 검사), `Receipt`, `PROTOCOL=1`.
- `crates/offline-core/src/store.rs`: `submit`(135줄)이 `Operation`을 만들고 `classroom`·`online` 전달 행을 만든다. `lease`(168), `accept_receipt`(184, `receipt.protocol!=PROTOCOL` 검사), `delivery_failed`(205), `pending`(111).
- `crates/offline-core/src/transport.rs`: `Transport` trait, `tick()`. 시험에서만 호출된다.
- `crates/offline-core/src/registration.rs`: `Registration`(18줄, `origin`, `trust`, 시험용 `transport: Option<Arc<dyn EnrollmentClient>>`), `fetch_lessons`·`fetch_role`(384~410줄, `#[cfg(windows)]` reqwest 설정)이 네트워크 호출의 기준.
- 자격 원문: `lesson_store.rs:69`처럼 `meta` 테이블 `key='grant'`의 `Signed`.
- 학생 앱: `apps/student/src-tauri/src/lib.rs`(명령 등록), `apps/student/ui/student.js:68`(`queue_submission` 호출, "수신 확인은 아직 받지 않았습니다" 문구), `apps/student/ui/index.html:76-77`.
- 신뢰 키: `release/native-policy.json`의 `online_receipt_keys`(현재 `{}`).

## 구현 결정

### 1. Operation protocol 2
- `contract.rs`에 `pub const OPERATION_PROTOCOL: u32 = 2;`. `PROTOCOL`(=1)은 Grant·Receipt·Lesson용으로 그대로 둔다.
- `Operation` 마지막 필드로 `#[serde(default, skip_serializing_if = "Option::is_none")] pub created_ms: Option<i64>` 추가. protocol 1 바이트는 바뀌지 않아야 한다.
- `Envelope::verify`: `protocol==1 && created_ms.is_none()` 또는 `protocol==2 && created_ms.is_some_and(|t| t>=0)`만 허용.
- `store.rs` `submit`: 새 Operation은 `protocol: OPERATION_PROTOCOL`, `created_ms: Some(now)`.
- `accept_receipt`의 `receipt.protocol!=PROTOCOL`은 그대로(Receipt는 1).
- `contracts/offline-v1.schema.json`의 Operation 정의에 `created_ms`(선택, 정수 ≥0)와 `protocol` enum [1,2] 반영. `contracts/README.md`에 protocol 2 한 단락 추가. 기존 fixture 파일은 바꾸지 않는다.

### 2. online receipt로 classroom 전달 정리
- `accept_receipt`에서 `expected == Online`으로 새 receipt를 저장할 때 같은 트랜잭션에서 `UPDATE deliveries SET state='superseded',lease=NULL,lease_until=0 WHERE operation_id=?1 AND destination='classroom' AND state IN ('pending','auth_wait','sending')`.
- `pending`의 제외 목록에 `'superseded'` 추가. 스키마(열·마이그레이션)는 바꾸지 않는다.

### 3. HTTP 전송
- 새 파일 `crates/offline-core/src/online_transport.rs`: `pub struct HttpTransport { origin: String, credential: Signed }`, `impl Transport`.
  - `#[cfg(windows)]`에서만 실제 전송. 그 밖에서는 `send`가 `Err(Error::VaultUnavailable)`(지금 `fetch_lessons`와 같은 방식).
  - reqwest 설정은 `fetch_role`과 같게(HTTPS 전용, 리다이렉트 금지, 연결 8초·전체 30초). 주소는 `format!("{origin}/api/offline/v1/submissions")`. 본문 `{"credential": credential, "envelope": envelope}`.
  - Destination이 `Online`이 아니면 `Err(Error::Invalid("destination"))`.
  - 응답 대응 (서버 카드와 같은 표):
    - 200 → 본문 `{"receipt": Signed}`(deny_unknown_fields, 64KiB 상한) → `Reply::Receipt`
    - 401 → `Reply::AuthenticationRequired`
    - 403, 409, 400 → `Reply::Conflict{remote_snapshot}`. 본문의 `remote_snapshot` 문자열이 있으면 그것, 없으면 `{"status":<숫자>,"code":<본문 code 또는 "">}`를 JSON 문자열로. 본문은 16KiB까지만 읽는다.
    - 429, 503 → `Reply::Retry{retry_after_ms}`(`Retry-After` 초 단위 정수만 해석, 그 외 None)
    - 그 밖의 상태·네트워크 오류 → `Err(Error::RegistrationFailed)`(코어가 backoff 재시도)
- `Registration`에 `pub fn deliver_online(&self) -> Result<DeliverySummary>`:
  - 잠금 해제·학생 앱·ReadWrite 상태가 아니면 바로 `Ok` 빈 요약(오류 아님).
  - `tick(store, transport, Destination::Online, now)`을 `Idle`이 되거나 20번이 될 때까지 반복. `AuthenticationRequired`·`Conflict`가 나와도 다음 항목으로 계속.
  - `DeliverySummary { received: u32, deferred: u32, auth_required: u32, conflicts: u32, remaining: u64 }`(remaining은 `pending(Online)`).
  - 시험을 위해 `Registration`에 `Transport`를 주입하는 경로를 둔다(기존 `EnrollmentClient` 주입과 같은 방식, `with_adapters` 확장 또는 별도 생성 함수).
  - 같은 `Registration`의 `operation` mutex로 동시 실행을 막는다.

### 4. 학생 앱
- `lib.rs`에 명령 `deliver_submissions`(spawn_blocking, `DeliverySummary` 반환)와 `submission_states(assignment) -> Vec<{operation_id, online_state, created_ms}>`(최근 5건, 상태는 deliveries의 online 행 state, `received`면 receipt의 `server_received_ms`도). 학생 capability 파일에만 추가한다.
- `student.js`:
  - `queue_submission` 성공 직후, 잠금 해제 직후, `online` 이벤트 때, 그리고 `pending_online > 0`인 동안 60초마다 `deliver_submissions` 호출. 실패는 화면에 조용히 "다음에 다시 보냅니다"로만 표시하고 입력은 유지.
  - 상태 문구(`answer-state`): 보관됨·보내는 중 → "제출 대기 중: 인터넷이 연결되면 보냅니다", received → "제출 완료(서버 수신 YYYY-MM-DD HH:MM)", auth_wait → "다시 로그인해야 보낼 수 있습니다", conflict → "서버에 이미 다른 제출이 있거나 마감되었습니다. 선생님께 확인해 주세요."
  - `index.html:77` 안내 문구는 "제출 대기열의 답안은 서버 수신 확인을 받으면 제출 완료로 바뀝니다."로.
- 정책 파일 `release/native-policy.json`의 `online_receipt_keys`는 **비워 둔다**(운영 키는 배포 때 넣는다). 비어 있으면 receipt 검증이 실패해 재시도 상태로 남는 것이 의도된 동작이다.

## 건드리지 말 것
- 교사·학부모 앱 명령과 capability
- 등록·자격·PIN·백업 로직, DB 스키마·마이그레이션 checksum
- 서명 도메인 문자열, 기존 fixture 파일
- `classroom` 전송 구현(2차 카드에서 다룬다)

## 테스트 (`crates/offline-core/tests/core.rs`에 추가, 가짜 `Transport` 사용)
- `protocol1_operation_bytes_unchanged`: `created_ms: None`인 Operation 직렬화에 `created_ms` 키가 없음, 기존 fixture envelope가 `verify` 통과
- `new_submissions_use_protocol2_with_created_ms`
- `envelope_verify_rejects_protocol_created_ms_mismatch`: (1, Some), (2, None), (3, _)
- `online_receipt_supersedes_classroom_delivery`: online receipt 후 `pending(Classroom) == 0`, classroom lease 없음
- `deliver_online_stops_when_idle_and_counts`: 가짜 전송이 Receipt·Retry·Conflict·AuthenticationRequired를 섞어 반환할 때 요약 값
- `deliver_online_is_noop_when_locked`
- `http_reply_mapping`: 상태 코드별 `Reply` 대응을 순수 함수(`map_response(status, retry_after, body) -> Result<Reply>`)로 뽑아 시험

## 완료 확인
- [ ] `cargo test --locked -p thinkgrow-offline-core` 통과
- [ ] `cargo check -p teacher -p student -p parent` 통과
- [ ] 위 테스트 존재·통과
- [ ] 커밋 1개
