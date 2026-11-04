# 설치판 전송 계약 v1

[JSON Schema](offline-v1.schema.json)와 [Python 서명 fixture](fixtures/python-signed-v1.json)는 온라인 API 구현 시 공동으로 사용하는 버전 고정 산출물이다. 기존 온라인 API가 이미 이 계약을 구현했다는 뜻은 아니다.

- 자격과 receipt는 Signed 객체의 payload UTF-8 원문을 서명한다. 검증 전 key_id는 native 설정의 허용 공개키에서만 찾는다. payload의 중복/미지원 필드는 거부한다.
- 서명 입력은 domain UTF-8 + NUL 1바이트 + payload다. domain은 자격 thinkgrow.credential.v1, 기기 작업 thinkgrow.operation.v1, receipt thinkgrow.receipt.v1 이다. 서명은 Ed25519 64바이트의 표준 Base64, 공개키는 32바이트 Base64다.
- Operation proof는 Rust Operation/Answer 선언 순서의 compact UTF-8 JSON에 서명한다. Python은 fixture와 같은 필드 순서, ensure_ascii=False, separators=(',', ':')를 사용한다. 부동소수점은 없다. 범용 JSON 객체를 임의 순서로 다시 직렬화하지 않는다.
- Operation protocol 2는 마지막 필드로 `created_ms`(기기 제출 시각, 정수 ≥0)를 추가한다. protocol 1 operation에는 `created_ms`가 없어야 하며 직렬화 바이트도 기존과 같다. protocol 2는 `created_ms`가 필수다. 마감 판정은 `created_ms`와 서버 도착 시각을 함께 쓰되 유예(72시간) 규칙은 서버가 정한다. Grant·Receipt·Lesson은 계속 protocol 1이다.
- payload_hash는 Answer의 같은 compact JSON 바이트에 대한 SHA-256이다. body는 opaque UTF-8 문자열이며 문항별 구조·평가 규칙은 이후 공유 평가 패키지에서 검증한다. 본문 2MiB·첨부 15MiB는 현재 코어의 안전 상한이며 온라인 최종 업로드 정책을 바꾸지 않는다.
- Envelope.verify는 기기 proof와 grant의 필드/범위 결속 검사다. 온라인 수신기는 먼저 issuer 서명·현재 자격/epoch·시간·배정·판본·마감·취소 정책을 검증해야 한다. 이 메서드만 호출하여 로그인/수신 승인을 만들지 않는다.
- 교실 relay는 원래 operation_id, origin_device_id, device_seq, proof를 유지한다. 목적지를 바꿔 새 작업을 생성하지 않는다. classroom receipt는 online receipt가 아니다.
- 직접 전송/교사 relay의 서버 중복 제거 장부는 origin_device_id + operation_id를 사용하고 같은 ID의 다른 payload를 거부해야 한다. 성공 HTTP 상태만으로 receipt를 생성하지 않는다.
- 기존 고정 작업은 자격 갱신 후에도 원래 생성 epoch/proof를 유지한다. 현재 권한으로 재전송을 허가하되 더 새로운 epoch를 사칭하는 원본은 거부한다. 기기키 회전 시 이전 키 이력·복구 경로는 별도 구현해야 한다.
- 예제 공개키/서명은 합성 fixture다. 제품 Trust에 등록하거나 운영 키로 사용하지 않는다.

릴리스 카탈로그는 별도 계약이며 Tauri 호환 Minisign 형식을 사용한다. 인증/기기/게임 키를 앱 릴리스 키로 재사용하지 않는다.

## 서명 수업 v1

prepared-lesson-v1.schema.json은 학생 텍스트 수업 7유형의 서명 payload다. 기존 offline-v1의 Answer.body에는 {format:1,body,structured_answer} JSON 문자열을 저장한다. protocol/schema 1 및 기존 DB migration checksum은 바꾸지 않는다. 상세 제한과 남은 첨부 지원은 ../docs/registration-and-lessons-2026-11-03.md를 따른다.
