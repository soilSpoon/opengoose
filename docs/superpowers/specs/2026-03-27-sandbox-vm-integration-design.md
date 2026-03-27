# Sandbox VM Integration Design

> **Date:** 2026-03-27
> **Status:** Draft
> **Goal:** Worker의 tool execution을 microVM에서 격리 실행하여 호스트 안전 보장

---

## 1. 배경

opengoose-sandbox 크레이트는 macOS HVF 기반 microVM을 구현했고, CoW fork (avg 164us), virtio-console JSON exec 파이프라인이 동작한다. 하지만 Worker와 통합되지 않아 실제 작업에 사용되지 않는 상태.

Worker가 자율적으로 코드를 작성/빌드/테스트할 때, 호스트 파일시스템에 직접 접근하는 것은 보안 위험이 있다. 특히 Wasteland 자율성(trust가 낮은 에이전트의 자율 실행)을 위해서는 격리가 필수.

### 현재 상태

- MicroVm: boot, snapshot, CoW fork, exec 동작
- SandboxPool: acquire/release, VM 재사용(reset)
- virtio-console: JSON `{"cmd":"exec","args":[...]}` 프로토콜
- virtio-fs: 미구현
- Worker 통합: 없음

---

## 2. 설계 결정

| 결정 사항 | 선택 | 이유 |
|-----------|------|------|
| 격리 범위 | Tool execution 프록시 (C 방식) | Agent 추론은 호스트(Goose 에코시스템 유지), shell/file 실행만 VM 격리 |
| 파일시스템 | virtio-fs | cargo build가 수천 번 syscall → 공유 마운트 필수. 명령 단위 I/O로는 불가 |
| 마운트 정책 | read-only 소스 + overlay | worktree를 RO 마운트, overlay(tmpfs)에 쓰기. 격리 원칙 준수 |
| 노출 범위 | worktree 디렉토리만 | 최소 노출. 의존성 캐시 공유는 후속 최적화 |
| 결과 추출 | git diff → git apply | 코드 변경만 정확 전달, 빌드 산출물 자동 제외 |
| 통합 방식 | SandboxClient (McpClientTrait) | BoardClient 패턴 동일. Goose 수정 없이 extension point 활용 |
| FUSE 범위 | Minimal (~15 ops) | cargo build/test 동작 수준. 부족한 ops는 ENOSYS 반환 후 필요시 추가 |

---

## 3. 아키텍처

### 3.1 Worker 파이프라인 변경

```
현재:
  claim → worktree → hydration → Agent.reply() → validation → submit → cleanup

변경 후:
  claim → worktree → VM fork(virtio-fs mount) → hydration
       → Agent.reply() (호스트, LLM 추론)
           ├─ shell tool call → SandboxClient → VM exec
           ├─ file write      → SandboxClient → VM exec
           └─ board tool call → BoardClient   → 호스트 직접
       → validation (VM 안에서 cargo test)
       → git diff (VM) → git apply (호스트 worktree)
       → submit → VM release → cleanup
```

### 3.2 VM 내부 구조

```
┌─────────────────────────────────────────┐
│ MicroVM (Alpine Linux, aarch64)          │
│                                          │
│  /workspace (overlayfs)                  │
│    lower = virtio-fs mount (read-only)   │
│    upper = tmpfs (read-write)            │
│    merged = /workspace                   │
│                                          │
│  도구: git, cargo, rustc, sh             │
│  통신: virtio-console (JSON exec)        │
│        virtio-fs (FUSE over virtqueue)   │
└──────────────┬──────────────┬────────────┘
               │              │
          virtio-fs      virtio-console
          (FUSE req)     (JSON cmd/result)
               │              │
┌──────────────┴──────────────┴────────────┐
│ VMM (opengoose-sandbox, 호스트)           │
│                                          │
│  FuseServer: FUSE 요청 → 호스트 syscall  │
│  SandboxClient: McpClientTrait 구현      │
│  SandboxPool: VM lifecycle 관리          │
└──────────────────────────────────────────┘
```

### 3.3 virtio-fs 구현

VMM 쪽에서 FUSE 서버를 구현한다. Guest 커널의 virtio-fs 드라이버가 FUSE 요청을 virtqueue로 보내면, VMM이 호스트 syscall로 변환한다.

**virtio-fs 디바이스 레이아웃:**
- virtqueue 2개: hiprio (FUSE_FORGET 등), request (일반 FUSE ops)
- MMIO 레지스터: 기존 virtio-console과 동일한 MMIO 주소 공간에 두 번째 디바이스로 추가
- device ID: 26 (virtio-fs)

**구현할 FUSE operations (~15개):**

| Operation | 용도 | 필수도 |
|-----------|------|--------|
| INIT | FUSE 프로토콜 핸드셰이크 | 필수 |
| LOOKUP | 경로에서 inode 찾기 | 필수 |
| GETATTR | 파일 메타데이터 (stat) | 필수 |
| OPENDIR | 디렉토리 열기 | 필수 |
| READDIR / READDIRPLUS | 디렉토리 내용 읽기 | 필수 |
| RELEASEDIR | 디렉토리 닫기 | 필수 |
| OPEN | 파일 열기 | 필수 |
| READ | 파일 읽기 | 필수 |
| RELEASE | 파일 닫기 | 필수 |
| CREATE | 파일 생성 | 필수 |
| WRITE | 파일 쓰기 | 필수 |
| MKDIR | 디렉토리 생성 | 필수 |
| UNLINK | 파일 삭제 | 필수 |
| RMDIR | 디렉토리 삭제 | 필수 |
| RENAME | 파일/디렉토리 이름 변경 | 필수 |
| STATFS | 파일시스템 통계 | 필수 (cargo가 호출) |
| FLUSH | 파일 flush | 있으면 좋음 |
| FSYNC | 파일 sync | 있으면 좋음 |
| DESTROY | 언마운트 | 있으면 좋음 |

나머지 ops(SETATTR, SYMLINK, READLINK, MKNOD, LINK, GETXATTR, LISTXATTR 등)는 ENOSYS를 반환한다. 필요 시 개별 추가.

**Inode 관리:**
- 호스트 파일의 `(dev, ino)` 쌍을 VM inode로 매핑
- `HashMap<u64, HostEntry>` — inode → 호스트 경로 + metadata
- root inode (1) = 마운트된 worktree 디렉토리

**read-only 강제:**
- FUSE 서버가 CREATE, WRITE, MKDIR, UNLINK, RENAME 요청에 대해 EROFS 반환
- overlay 계층(guest 내부 tmpfs)이 이 에러를 잡아서 upper layer에 쓰기

### 3.4 SandboxClient

`McpClientTrait`을 구현하여 Goose Agent에 sandbox tool을 제공한다.

```rust
pub struct SandboxClient {
    info: InitializeResult,
    pool: Arc<SandboxPool>,
    vm: Mutex<Option<MicroVm>>,   // 현재 작업용 VM
    worktree_path: PathBuf,        // 호스트 worktree 경로
}

// 제공하는 도구:
// - sandbox_exec: VM에서 명령 실행 (shell command)
// - sandbox_write_file: VM overlay에 파일 쓰기
// - sandbox_read_file: VM 파일시스템에서 파일 읽기
```

**도구 호출 흐름:**

1. Agent가 `sandbox_exec(cmd: "cargo test")` 호출
2. SandboxClient가 VM의 virtio-console로 JSON 전달
3. VM 안에서 overlay된 /workspace에서 `cargo test` 실행
4. 결과(stdout, stderr, exit code)를 JSON으로 반환
5. SandboxClient가 `CallToolResult`로 변환하여 Agent에게 전달

### 3.5 결과 추출 (git diff → apply)

작업 완료 후:

1. SandboxClient가 VM에서 `git diff` 실행 (`sandbox_exec("git", ["diff", "--no-color"])`)
2. diff 출력을 호스트로 가져옴
3. 호스트 worktree에서 `git apply` 실행
4. 적용 성공 시 submit, 실패 시 에러 보고

**Edge cases:**
- 새 파일 추가: `git diff`는 untracked 파일을 포함하지 않음 → `git add -N .` 후 diff, 또는 `git diff HEAD` + `git ls-files --others`로 보완
- 바이너리 파일: diff에 포함 안 됨 → 코드 변경만 전달하므로 정상 동작
- 빈 diff: validation 통과했지만 변경 없음 → submit (no-op 작업)

### 3.6 Guest Initramfs 확장

현재 initramfs는 minimal (busybox + custom init). 다음을 추가:

- `git`: 결과 추출용
- `cargo` + `rustc`: 빌드/테스트용 (Alpine APK)
- `sh` (busybox): 이미 포함

initramfs 크기가 커지면 부팅 시간에 영향. 두 가지 전략:
1. **Fat snapshot**: 도구 포함된 상태에서 snapshot → fork. 부팅은 느리지만 fork 후 바로 사용.
2. **Lazy install**: 기본 snapshot에서 fork 후 APK install. fork는 빠르지만 매 실행마다 설치 오버헤드.

**선택: Fat snapshot.** snapshot은 한 번만 만들고 이후 fork는 164us. initramfs에 도구를 포함하는 게 합리적.

---

## 4. Worker 통합

### 4.1 process_claimed_item 변경

```rust
async fn process_claimed_item(&self, item: &WorkItem, board: &Arc<Board>, repo_dir: &Path) {
    // Phase 1: Worktree (기존)
    let guard = self.acquire_worktree(repo_dir, item.id)?;

    // Phase 2: Sandbox VM (NEW)
    let sandbox = if self.sandbox_enabled() {
        let pool = self.sandbox_pool()?;
        let mut vm = pool.acquire()?;
        vm.mount_virtio_fs(&guard.path)?;  // worktree를 RO 마운트
        Some(SandboxContext { vm, pool })
    } else {
        None
    };

    // Phase 3: Agent.reply() — tool calls이 SandboxClient를 통해 VM으로 감
    // Phase 4: Validation — sandbox가 있으면 VM에서 실행
    // Phase 5: git diff → apply (sandbox가 있으면)
    // Phase 6: submit
    // Phase 7: VM release + worktree cleanup
}
```

### 4.2 Sandbox 토글

모든 Worker가 sandbox를 쓰는 게 아니라, 설정으로 토글:

- 환경변수: `OPENGOOSE_SANDBOX=1`
- 향후: Board의 rig trust level에 따라 자동 결정 (trust < threshold → sandbox 강제)

sandbox가 비활성이면 기존 파이프라인 그대로 동작 (worktree에서 직접 실행).

---

## 5. 구현 Phase

### Phase 1: virtio-fs (~핵심, 가장 큰 작업)

opengoose-sandbox 크레이트에 추가:

1. **VirtioFs 디바이스** — virtqueue setup, MMIO 레지스터, feature negotiation
2. **FuseServer** — FUSE 요청 디코딩, 호스트 syscall 실행, 응답 인코딩
3. **Inode 테이블** — 호스트 파일 ↔ guest inode 매핑
4. **MicroVm 통합** — 두 번째 virtio 디바이스로 추가, MMIO 라우팅
5. **Guest 측** — initramfs에 virtio-fs mount 스크립트 (`mount -t virtiofs tag /workspace`)
6. **Overlay 셋업** — guest init에서 overlayfs 마운트 (`mount -t overlay overlay -o lowerdir=/workspace,upperdir=/tmp/upper,workdir=/tmp/work /workspace`)
7. **테스트** — 호스트 디렉토리를 마운트하고 ls, cat, 파일 쓰기(overlay) 확인

### Phase 2: SandboxClient + 결과 추출

opengoose 바이너리 크레이트에 추가 (opengoose-sandbox 의존성은 바이너리에만):

1. **SandboxClient** — `McpClientTrait` 구현, sandbox_exec/read/write 도구. opengoose 바이너리 크레이트에 위치 (opengoose-rig가 opengoose-sandbox에 의존하지 않도록)
2. **Agent factory 확장** — sandbox 활성 시 SandboxClient를 Agent에 주입
3. **Git diff 추출** — VM에서 diff 생성, 호스트에서 apply
4. **Guest initramfs 확장** — git, cargo, rustc 추가 (fat snapshot)

### Phase 3: Worker 통합

opengoose 바이너리 + opengoose-rig에서:

1. **process_claimed_item 수정** — sandbox 분기 추가
2. **ValidationGate 수정** — sandbox 있으면 VM에서 cargo test
3. **환경변수 토글** — `OPENGOOSE_SANDBOX=1`
4. **성능 벤치마크** — sandbox on/off 비교
5. **에러 처리** — VM 장애 시 fallback (worktree 직접 실행 또는 abandon)

---

## 6. 테스트 전략

| 레벨 | 대상 | 방법 |
|------|------|------|
| Unit | FuseServer ops 개별 | 호스트 tmpdir에서 FUSE 요청 시뮬레이션 |
| Unit | Inode 테이블 | 매핑 생성/조회/삭제 |
| Unit | SandboxClient | mock VM으로 도구 호출 테스트 |
| Integration | virtio-fs 마운트 | VM fork → mount → ls /workspace → 파일 내용 확인 |
| Integration | overlay 쓰기 | VM에서 파일 수정 → git diff 추출 → 원본 불변 확인 |
| Integration | 전체 파이프라인 | Board.post → Worker claim → VM exec → git apply → submit |
| Benchmark | fork + mount 지연 | SandboxPool.acquire() + virtio-fs mount 시간 측정 |

macOS 전용 (`#[cfg(target_os = "macos")]`). CI에서는 sandbox 테스트를 macOS runner에서만 실행.

---

## 7. 범위 외 (명시적 제외)

- DAX (Direct Access): virtio-fs 성능 최적화. 첫 버전에서는 일반 read/write.
- 의존성 캐시 공유 (`~/.cargo/registry`): 후속 최적화.
- 네트워크 격리: guest에 네트워크 없음 (이미 기본).
- 멀티 VM: SandboxPool이 현재 단일 VM 캐시. 멀티 Worker 지원은 별도 설계.
- Linux 지원: macOS HVF 전용. Linux KVM 지원은 후속.
- Full FUSE spec: ~15 ops만 구현. 나머지는 ENOSYS.
