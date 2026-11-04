# 학생 제출 전송 설계 (2026-11-04)

## 결정 사항

- 마감 판정: **기기 제출 시각 + 도착 유예 72시간.** 기기에서 마감 전에 고정한 제출본이 마감 후 72시간 안에 서버에 도착하면 정시 제출로 인정하고, 교사 화면에 "오프라인 제출·늦게 도착"을 표시한다.
- 범위: **온라인 직접 전송과 교실 relay 모두.** 두 차례로 나눠 구현한다.
- 구현: OpenCode(방식 B). 카드마다 검수한다.

## 현재 상태

| 구간 | 있음 | 없음 |
|---|---|---|
| 학생 앱 저장 | 초안 자동저장, 고정 제출본 + outbox 원자 저장(`store.rs` `submit`). 제출마다 `classroom`, `online` 전달 행 2개 생성. 화면은 "대기열 보관" | 실제 전송 호출 |
| 전송 코어 | `transport.rs` `tick()`: lease, 재시도·backoff·Retry-After, 인증 대기, 충돌 보존, 서명 receipt 검증 후에만 완료 | `Transport` 구현체. `tick()`은 시험에서만 호출 |
| 신뢰 설정 | `release/native-policy.json`의 `online_receipt_keys`, `classroom_receipt_keys`(현재 비어 있음) | 실제 receipt 공개키 |
| 서버 | 기기 등록·자격 서명(`offline_devices.py`), 서명 요청 인증(`offline_lessons.authenticate`), 교사 채점 오프라인 반영 중복 장부(`assessment_data.grade(offline=...)`) | 학생 제출 수신 API, receipt 서명키 |
| 교실 LAN | 계약상 relay 규칙(원 operation 유지, classroom receipt는 online receipt가 아님) | 교사 기기 수신기, 학생 LAN 클라이언트, 교사 기기 키 인증 |
| 계약 | `Operation`에 기기 제출 시각 없음 | 마감 판정용 시각 |

현재 네트워크 호출(`fetch_lessons`, `fetch_role`)은 `#[cfg(windows)]`에서만 동작한다. Android 학생 앱 전송은 이 설계 범위 밖이다.

## 계약 변경: Operation protocol 2

- `Operation`에 `created_ms: Option<i64>`를 **마지막 필드**로 추가한다(`#[serde(default, skip_serializing_if="Option::is_none")]`).
- 새 제출은 `protocol: 2`, `created_ms` 필수. 이미 대기열에 있는 protocol 1 제출은 바이트가 그대로라 서명이 유지되며, 서버 도착 시각으로 판정한다.
- Receipt·Grant·Signed 형식과 `PROTOCOL`(=1) 의미는 그대로 둔다. Operation 전용 상수 `OPERATION_PROTOCOL: u32 = 2`를 둔다.

## 1차: 학생 기기 → 온라인 직접 전송

카드: 플랫폼 `.mece/cells/PLT-SUB-01/TASK.md`, 오프라인 앱 `docs/tasks/OFF-SUB-01.md`. 서로 독립이라 동시에 진행한다.

### 서버 `POST /api/offline/v1/submissions`

- 요청 `{"credential": Signed(Grant), "envelope": Envelope}`, 본문 상한 2.5MiB.
- 검증: 자격 서명·유효 기간 → 기기 proof(`thinkgrow.operation.v1`, Rust 필드 순서 compact JSON) → `payload_hash` → `offlineDevices` 기기 상태·epoch → 자격 scopes와 학급 연결.
- 저장: `save_answer`에 `offline=` 인자를 더해 한 트랜잭션에서 처리하고, 장부 `offlineSubmissionOperations/{origin_device_id}__{operation_id}`에 hash와 receipt를 남긴다. 같은 ID로 다시 오면 저장된 receipt를 그대로 반환한다.
- 이미 웹에서 제출된 답안이 있으면 충돌로 돌려준다(먼저 제출한 쪽 유지).
- 응답 대응

| 서버 | 클라이언트 `Reply` |
|---|---|
| 200 `{"receipt": Signed}` | `Receipt` |
| 401 | `AuthenticationRequired` |
| 409 `OFFLINE_CONFLICT`/`ALREADY_SUBMITTED`/`DEADLINE_EXCEEDED` 등 + `remote_snapshot` | `Conflict` |
| 403 기기 철회·범위 거부 | `Conflict`(자동 재시도 안 함) |
| 429·503 (+`Retry-After`), 409 `CONCURRENT_MODIFICATION` | `Retry` |

- receipt 서명키는 자격 서명키와 분리(`THINKGROW_OFFLINE_RECEIPT_KEY`, `THINKGROW_OFFLINE_RECEIPT_KEY_ID`). `capabilities`에 `online_receipt_keys` 추가.

### 학생 앱

- `HttpTransport`(Windows), 전송 반복기, 화면 상태 표시.
- online receipt를 받으면 같은 제출의 `classroom` 전달은 `superseded`로 바꿔 더 보내지 않는다.

## 2차: 교실 relay (1차 검수 뒤 카드 작성)

1. **교사 기기 교실 키 인증.** 교사 앱이 교실 receipt 서명키(Ed25519)와 LAN TLS 키를 만들고, `roles/exchange` 새 동작으로 서버에 등록한다. 서버는 `ClassroomCert {teacher_device_id, teacher_id, class_ids, receipt_public_key, tls_spki_sha256, issued_ms, expires_ms≤7일}`를 `thinkgrow.classroom.v1`로 서명한다.
2. **학생에게 인증서 배포.** `lessons/prepare` 응답에 학생 학급 담당 교사들의 `ClassroomCert`를 함께 준다. 학생 앱은 검증한 인증서로 classroom receipt 신뢰 키를 채운다(정적 정책 파일 대신).
3. **교실 수신.** 교사 앱에서 "교실 수신"을 켠 동안만 LAN 포트를 연다. 화면에 주소와 QR을 보여 주고, 학생은 주소를 입력하거나 QR을 읽는다. TLS는 교사 기기 자체 인증서를 쓰고, 학생은 인증서의 SPKI 해시로 고정 검증한다(새 의존성 rustls는 AGENTS.md 조사 규칙에 따라 비교 후 결정).
4. **교사 기기 저장·재전송.** 교사 앱은 학생 자격·기기 proof·학급 범위를 확인하고 암호화해 보관한 뒤 classroom receipt를 준다. 인터넷이 되면 `POST /api/offline/v1/submissions/relay`(교사 자격·proof + 학생 자격 + 원 envelope + classroom receipt)로 보낸다. 서버는 같은 장부로 중복을 막고 online receipt를 준다.
5. **receipt 되돌려 주기.** 학생이 다시 교실에 연결하면 교사 기기가 받아 둔 online receipt를 내려준다. 학생 앱은 online 키로 검증해 인터넷 없이도 제출 완료로 바꾼다.
6. 마감 판정에서 relay 제출은 `created_ms`가 classroom receipt 시각 + 10분 이하인지 함께 확인한다.

## 배포 순서

1. receipt 키 생성 → Secret Manager 등록(사용자 승인).
2. 플랫폼 배포(수신 API). 앱이 아직 보내지 않으므로 영향 없음.
3. 정책 파일에 공개키 반영 → 학생 앱 빌드·배포(FIX-OFF-01 포함).
4. 2차는 같은 순서로 반복(교사 앱 포함).
