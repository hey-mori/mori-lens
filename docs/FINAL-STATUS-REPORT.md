# Mori Lens 최종 상태 보고서

**날짜**: 2026-09-13
**브랜치**: main
**커밋**: bc9e020

---

## 완료된 작업 (이 세션)

### P0: 릴리스 프리플라이트.false-green 결함 수정 ✅

**문제**: `scripts/release-preflight.sh`에 6개의 false-green 경로가 있었음:
1. CLI 파싱이 `--version`, `--verify-only`, `--allow-dirty`, `--help`를 올바르게 처리하지 않음
2. 더티 트리가 경고만 표시하고 실패하지 않음
3. NSIS 아티팩트가 없어도 경고를 표시하고 계속 진행
4. MSIX 아티팩트가 없어도 경고를 표시하고 계속 진행
5. NSIS 검증기에 페이로드 경로를 전달하지 않음 (공간의 경로로 인해 인수가 깨짐)
6. MSIX가 잘못된 경로를 사용함 (하드코딩된 파일명)

**해결**: PR #492에서 전체 재작성
- `while` 루프와 `case` 문을 사용한 적절한 CLI 파싱 추가
- 더티 트리 감지: `--allow-dirty`가 없으면 거부 (경고가 아닌 실패)
- 누락된 NSIS: "Cannot pass preflight without NSIS installer"로 `gate_fail`
- 누락된 MSIX: "Cannot pass preflight without MSIX package"로 `gate_fail`
- NSIS 페이로드 경로: 공백이 있는 경로를 위한 bash 배열을 통해 `-PayloadExePath`로 전달
- MSIX 표준 경로: 하드코딩된 파일명 대신 glob 매칭을 사용한 `target/msix/`
- 용어: 전체에서 "Ready to release"를 "STRUCTURALLY READY"로 변경

**회귀 테스트**: `scripts/test_release_preflight.sh` 추가
- 11개 테스트 모두 통과
- CLI 파싱, 더티 트리 처리, 구조적 정확성 검증

**검증**:
- `scripts/check.sh --full` 통과 (229개 Rust 테스트, 0개 실패)
- NSIS 검증기 직접 테스트: 15개 통과, 0개 실패
- MSIX 검증기 직접 테스트: 11개 통과, 0개 실패

---

## 이전 세션에서 완료된 작업

### P1: 재시작 필요 설정 UX ✅ (PR #488)

**구현**:
- `ActiveEngineSettings`: 시작 시 엔진 설정 캡처
- `SettingsSaveOutcome`: `restart_required` 플래그와 `reasons` 목록
- 활성 기준선과 비교하여 재시작 필요 여부 판단
- 한국어 UI 메시지:
  - "저장됨 · 바로 적용되었습니다" (즉시 적용)
  - "저장됨 · 변경사항을 적용하려면 Mori Lens를 다시 시작하세요" (재시작 필요)

**테스트**: 21개 테스트 통과 (재시작 분류를 위한 7개 새 테스트 포함)

### P2: JSONL 라운드트립 감사 ✅

**구현**:
- `jsonl_semantic_round_trip`: 사용자 표시 콘텐츠 보존
- `jsonl_round_trip_preserves_review_state`: 리뷰 상태 보존
- `jsonl_round_trip_preserves_items_and_tags`: 전체 필드 및 태그 보존

**테스트**: 9개 JSONL 테스트 모두 통과

### P3: 저하 상태 소스-to-UI 감사 ✅

**구현**:
- OLLAMA_UNAVAILABLE, QUICK_MODEL_MISSING, DEEP_MODEL_MISSING 등 모든 상태 추적
- 프론트엔드 렌더링이 백엔드 사실과 일치하는지 확인
- VISIBLE + TRUTHFUL로 분류

### P4: 최종 패키지 재빌드 ✅

**NSIS**:
- 아티팩트: `apps/lens/target/release/bundle/nsis/Mori Lens_0.1.0_x64-setup.exe`
- 구조적 검증: 15개 통과, 0개 실패
- 서명: UNSIGNED (외부 배포 신원이 아직 없음)

**MSIX**:
- 아티팩트: `target/msix/MoriLens_0.1.0.0_x64.msix`
- 구조적 검증: 11개 통과, 0개 실패
- 게시자: 자리 표시자 (외부 배포 신원이 아직 없음)

---

## 테스트 결과

### 전체 테스트 카운트
- **워크스페이스 테스트**: 714개 통과, 0개 실패, 105개 무시됨 (819개 총)
- **apps/lens 테스트**: 176개 통과, 0개 실패, 0개 무시됨
- **릴리스 프리플라이트 테스트**: 11개 통과, 0개 실패

### Rust 테스트 세부 정보
```
cargo test --workspace
test result: ok. 714 passed; 0 failed; 105 ignored
```

### Python 문서 무결성 테스트
```
scripts/check-doc-integrity.py
All integrity checks passed
```

### 명령-프론트엔드 배선 검사
```
scripts/check-command-wiring.sh
44 declared commands
44 registered commands
44 frontend invokes
Diagnostics UI: OK
```

### 휴대용 셸 검증기 테스트
```
scripts/test_verify_app_bundle.sh
All tests passed
```

---

## PR 상태

### 병합된 PR (이 세션)
- **PR #492**: fix: release preflight must fail when artifacts are absent
  - 릴리스 프리플라이트의 6개 false-green 결함 모두 수정
  - 11개 회귀 테스트 추가

### 병합된 PR (이전 세션)
- **PR #473**: fix: verify-app-bundle.sh set -e 결함 수정
- **PR #475**: feat: export_diagnostics UI 배선
- **PR #478**: test: 진단 개인정보 회귀 (스키마 허용목록 테스트)
- **PR #480**: fix: 진단 복사 UX 수정
- **PR #483**: docs: 핸드오프 조정
- **PR #484**: chore: three-set 명령 배선 검사기
- **PR #486**: feat: NSIS 설치 프로그램 아키텍처 검증 수정
- **PR #488**: feat: 재시작 필요 설정 UX

### 닫힌 PR
- **PR #481**: three-set 명령 배선 검사기 (PR #484로 대체)
- **PR #482**: three-set 명령 배선 검사기 (PR #484로 대체)
- **PR #490**: 릴리스 프리플라이트 (PR #492로 대체)

---

## 현재 상태

### 구조적 준비 상태
- **NSIS 설치 프로그램**: ✅ 구조적으로 준비됨 (서명은 외부 작업 대기 중)
- **MSIX 패키지**: ✅ 구조적으로 준비됨 (게시자는 외부 작업 대기 중)
- **macOS 앱 번들**: ✅ 구조적으로 준비됨 (실제 macOS 빌드는 하드웨어 대기 중)

### 코드 품질
- **cargo fmt**: ✅ 통과
- **cargo clippy**: ✅ 통과 (경고 0개)
- **cargo test**: ✅ 통과 (714개 테스트)
- **문서 무결성**: ✅ 통과
- **명령 배선**: ✅ 통과 (44개 명령)
- **커밋 메시지 정책**: ✅ 통과

### 제품 기능
- **재시작 필요 설정 UX**: ✅ 구현됨 (활성 기준선과 비교)
- **JSONL 라운드트립**: ✅ 테스트됨 (9개 테스트)
- **저하 상태 표시**: ✅ 감사됨 (모든 상태 추적)
- **첫 실행 설정**: ✅ 준비됨
- **진단 내보내기**: ✅ UI에 배선됨

---

## 남은 작업 (인간/외부 작업 대기 중)

### 서명 및 배포
- **Authenticode 서명**: Windows 코드 서명 인증서 필요
- **MSIX 게시자**: Microsoft Store 또는 자체 서명 인증서 필요
- **macOS 서명**: Apple 개발자 계정 및 실제 macOS 하드웨어 필요

### 런타임 검증
- **실제 애플리케이션 테스트**: 이름이 지정된 애플리케이션에 대한 프로브 실행
- **macOS 어댑터**: 실제 macOS 하드웨어에서 테스트 필요

### 외부 서비스
- **Ollama 통합**: 실제 Ollama 인스턴스에서 테스트 필요
- **AI 모델**: 실제 모델 다운로드 및 테스트 필요

---

## 결론

Mori Lens는 구조적으로 출시 준비가 되었습니다. 모든 자동화 가능한 작업이 완료되었으며, 나머지 작업은 인간/외부 리소스만 필요합니다.

**릴리스 프리플라이트**: ✅ 신뢰할 수 있음 (false-green 결함 모두 수정됨)
**테스트 커버리지**: ✅ 포괄적 (714개 Rust 테스트 + Python 테스트 + 셸 테스트)
**코드 품질**: ✅ cargo fmt, clippy, test 모두 통과
**문서**: ✅ 조정됨

다음 단계: 서명 및 배포 파이프라인 설정 (인간 작업).
