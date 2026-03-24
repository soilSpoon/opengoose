# opengoose-sandbox Design Spec

## Overview

Sub-millisecond microVM sandboxing for opengoose agent tool execution. A minimal VMM that boots a Linux guest on Apple Hypervisor.framework, snapshots it, and forks via CoW memory mapping.

**Current target:** macOS Apple Silicon (aarch64, HVF) only. Extensible to Linux KVM via Hypervisor trait.

## Problem

Goose agent가 실행하는 도구(셸 커맨드, 파일 조작, 빌드/테스트)를 격리 없이 호스트에서 직접 실행하면 위험하다. 기존 샌드박스(컨테이너, BoxLite ~50ms, microsandbox ~200ms)는 매번 부팅하므로 느리다.

Zeroboot가 증명한 패턴: 이미 부팅된 VM 메모리를 `mmap(MAP_PRIVATE)`로 CoW 공유하면 fork당 ~1ms, 인스턴스당 ~265KB.

## Key Decisions

| 결정 | 선택 | 이유 |
|---|---|---|
| VMM | 자체 최소 VMM | libkrun은 C API가 스냅샷 미노출, fork하면 불필요한 코드가 너무 많음 |
| 하이퍼바이저 (macOS) | HVF via `extern "C"` + framework 링킹 | 정적 링킹이 가장 단순. macOS 전용이므로 동적 로딩 불필요 |
| 하이퍼바이저 (Linux) | 미구현 (trait만) | 현재 실행 환경은 macOS. kvm-ioctls로 추후 확장 |
| 커널 | libkrunfw | 이미 macOS/Linux용 최소 커널을 빌드해둔 프로젝트 |
| 스냅샷 생성 | 자체 (Firecracker 의존 없음) | Firecracker는 Linux 전용 + vmstate 포맷 불안정 |
| 스냅샷 포맷 | serde + bincode | 레지스터를 직접 읽으므로 파싱 휴리스틱 불필요 |
| 스냅샷 시점 | 첫 사용 시 자동 생성, 로컬 캐시 | 별도 init 명령 불필요 |
| CoW 방식 | 메모리 레벨 (mmap MAP_PRIVATE) | 디스크 CoW(QCOW2)나 OverlayFS보다 수백 배 빠름 |
| 동시성 | 순차 재사용 (pool_size=1) | HVF 프로세스당 VM 1개 제약. 추후 KVM에서 병렬 확장 |
| 네트워크 | 미지원 | 당장 불필요. 확장 가능하게 설계만 |
| 호스트 파일 공유 | 미지원 | virtio-fs 추후 추가 가능 |

## Architecture

```
opengoose-sandbox
│
├── hypervisor/
│   ├── trait Hypervisor        VM 생성/삭제, 메모리 매핑
│   ├── trait Vcpu              레지스터 읽기/쓰기, run, pause
│   └── hvf.rs                  HVF 구현 (extern C + framework linking)
│                               (추후 kvm.rs 추가)
│
├── machine.rs                  ARM64 머신 정의: 메모리 맵, DTB 생성, GIC 설정
│
├── boot.rs                     linux-loader + libkrunfw로 VM 부팅
│
├── snapshot.rs                 스냅샷 생성/로드/CoW 매핑/디스크 캐시
│   ├── create()                부팅 → pause → 레지스터 + 메모리 저장
│   ├── load()                  캐시된 파일 로드
│   ├── save_to_disk()          ~/.opengoose/snapshots/ 에 저장
│   └── cow_map()               mmap(MAP_PRIVATE | MAP_NORESERVE)
│
├── vm.rs                       MicroVm (fork된 인스턴스)
│   ├── fork_from(snapshot)     CoW fork + 레지스터 복원
│   ├── exec(cmd) -> Result     시리얼로 명령 → 결과 수신
│   └── Drop                    munmap + VM 정리
│
├── uart.rs                     PL011 UART 에뮬레이션
│
├── pool.rs                     SandboxPool (순차 재사용)
│   └── acquire()               캐시된 스냅샷 없으면 자동 생성
│
├── lib.rs                      공개 API
└── error.rs                    SandboxError
```

Guest binary (별도, 워크스페이스 외부):
```
guest/init/                     musl 정적 바이너리
└── src/main.rs                 시리얼에서 명령 대기 → exec → 결과 반환
```

## ARM64 Machine Definition

### Memory Map

```
0x0000_0000 - 0x0800_0000   Flash/reserved (128 MiB)
0x0800_0000 - 0x0801_0000   GICv3 Distributor (64 KiB)
0x0801_0000 - 0x0802_0000   GICv3 Redistributor (64 KiB)
0x0900_0000 - 0x0900_1000   PL011 UART (4 KiB)
0x4000_0000 - ...            RAM start (guest memory)
```

RAM 크기: 기본 128 MiB. 커널은 RAM 시작 주소(0x4000_0000)에 로드.

### Device Tree Blob (DTB)

`vm-fdt` crate으로 런타임 생성. 최소 노드:

- `/` — compatible, model, #address-cells, #size-cells
- `/memory` — RAM 시작/크기
- `/cpus/cpu@0` — compatible = "arm,arm-v8"
- `/intc` — GICv3 (compatible = "arm,gic-v3"), distributor + redistributor 주소
- `/uart@9000000` — PL011 (compatible = "arm,pl011"), MMIO 주소, interrupt 번호
- `/timer` — ARM architectural timer, interrupt 번호

DTB는 RAM 끝 근처에 배치. 부팅 시 X0 = DTB 주소.

### GIC (Generic Interrupt Controller)

HVF: `hv_gic_create()` + `hv_gic_config()` (macOS 15+)로 in-kernel GIC 에뮬레이션.
KVM (추후): `KVM_CREATE_DEVICE` + `KVM_DEV_TYPE_ARM_VGIC_V3`.

GIC가 없으면 타이머 인터럽트와 UART 인터럽트가 전달되지 않아 게스트가 멈춤.

## Hypervisor Trait

```rust
pub trait Hypervisor: Send + Sync {
    type Vm: Vm;
    fn create_vm(&self) -> Result<Self::Vm>;
}

pub trait Vm: Send {
    type Vcpu: Vcpu;
    fn map_memory(&mut self, gpa: u64, host_addr: *mut u8, size: usize) -> Result<()>;
    fn unmap_memory(&mut self, gpa: u64, size: usize) -> Result<()>;
    fn create_vcpu(&mut self) -> Result<Self::Vcpu>;
    fn create_gic(&mut self, config: GicConfig) -> Result<()>;
}

pub trait Vcpu: Send {
    fn get_reg(&self, id: VcpuReg) -> Result<u64>;
    fn set_reg(&mut self, id: VcpuReg, val: u64) -> Result<()>;
    fn get_sys_reg(&self, id: SysReg) -> Result<u64>;
    fn set_sys_reg(&mut self, id: SysReg, val: u64) -> Result<()>;
    fn get_all_regs(&self) -> Result<VcpuState>;
    fn set_all_regs(&mut self, state: &VcpuState) -> Result<()>;
    fn run(&mut self) -> Result<VcpuExit>;
}

/// Bulk state for snapshot save/restore
#[derive(Serialize, Deserialize)]
pub struct VcpuState {
    pub regs: Vec<(VcpuReg, u64)>,          // X0-X30, SP, PC, PSTATE
    pub sys_regs: Vec<(SysReg, u64)>,       // see below
}

pub enum VcpuExit {
    MmioRead { addr: u64, len: u8 },
    MmioWrite { addr: u64, data: Vec<u8> },
    SystemEvent,
    Hlt,
    Unknown(u32),
}
```

### ARM64 System Registers (스냅샷에 필요한 최소 목록)

```
SCTLR_EL1       — System control (MMU enable, caches)
TCR_EL1          — Translation control
TTBR0_EL1        — Translation table base 0
TTBR1_EL1        — Translation table base 1
MAIR_EL1         — Memory attribute indirection
VBAR_EL1         — Vector base address
ESR_EL1          — Exception syndrome
FAR_EL1          — Fault address
SP_EL1           — Stack pointer (EL1)
ELR_EL1          — Exception link register
SPSR_EL1         — Saved program status
CNTV_CTL_EL0     — Virtual timer control
CNTV_CVAL_EL0    — Virtual timer compare value
CNTVOFF_EL2      — Virtual timer offset
```

### HVF Exit Parsing

`hv_vcpu_run()` 반환 후 `hv_vcpu_get_exit_info()` → `hv_vcpu_exit_t`:
- `exception.syndrome` (ESR_EL2) 디코딩:
  - EC `0x24` (Data Abort) → MMIO read/write (UART 접근 시 발생)
  - EC `0x01` (WFI/WFE trap) → Hlt
  - EC `0x16` (HVC) → System event
- `exception.virtual_address` + syndrome ISS 필드에서 MMIO 주소와 읽기/쓰기 방향 추출

## PL011 UART Emulation

ARM64 표준 UART. libkrunfw 커널이 기대하는 디바이스.

MMIO 주소: `0x0900_0000` (4 KiB 범위)

에뮬레이션할 레지스터:
- `UARTDR` (0x000) — 데이터 읽기/쓰기
- `UARTFR` (0x018) — 플래그 (TX empty, RX ready)
- `UARTIMSC` (0x038) — 인터럽트 마스크
- `UARTRIS` (0x03C) — Raw 인터럽트 상태
- `UARTMIS` (0x040) — Masked 인터럽트 상태
- `UARTICR` (0x044) — 인터럽트 클리어

동작:
- 게스트가 `UARTDR`에 쓰기 → 호스트 출력 버퍼에 바이트 추가
- 게스트가 `UARTDR` 읽기 → 호스트 입력 버퍼에서 바이트 제공
- 게스트가 `UARTFR` 읽기 → TX always empty, RX ready if input buffer non-empty

## Snapshot Format

```rust
#[derive(Serialize, Deserialize)]
pub struct VmSnapshot {
    pub arch: Arch,                     // Aarch64 (추후 X86_64)
    pub vcpu_state: VcpuState,          // 일반 + 시스템 레지스터
    pub mem_size: usize,
    pub kernel_hash: String,            // 캐시 무효화용
}
```

메모리는 별도 파일 (`snapshot.mem`)로 저장. 스냅샷 메타데이터 (`snapshot.meta`)와 분리 — 메모리 파일은 직접 mmap 대상이므로 직렬화하지 않는다.

저장 위치: `~/.opengoose/snapshots/aarch64/snapshot.{meta,mem}`

## CoW Fork Flow

```
[스냅샷 로드]
mem_file = open("snapshot.mem")
cow_mem = mmap(NULL, mem_size, PROT_READ|PROT_WRITE,
               MAP_PRIVATE | MAP_NORESERVE, mem_file, 0)
  → 읽기: 원본 페이지 공유 (물리 메모리 0)
  → 쓰기: 해당 페이지만 복사 (CoW, ~4KB per page)

[VM 생성]
hv_vm_create()
hv_vm_map(cow_mem, 0x4000_0000, mem_size, HV_MEMORY_READ|WRITE|EXEC)
hv_gic_create()  // GIC 설정
vcpu = hv_vcpu_create()

[레지스터 복원]
vcpu.set_all_regs(&snapshot.vcpu_state)

[실행]
hv_vcpu_run(vcpu)  → 게스트가 부팅된 시점에서 바로 재개
```

## Serial I/O Protocol

호스트 ↔ 게스트 통신은 PL011 UART 시리얼. Newline-delimited JSON:

```json
// 호스트 → 게스트 (명령)
{"cmd":"exec","args":["ls","-la","/tmp"]}

// 게스트 → 호스트 (결과)
{"status":0,"stdout":"...","stderr":"..."}
```

게스트 init은 시리얼에서 JSON 한 줄 읽기 → exec → 결과 JSON 쓰기를 반복.

## Guest Init

`guest/init/` — 독립 Cargo 프로젝트 (워크스페이스 외부, `aarch64-unknown-linux-musl` 타겟).

```rust
fn main() {
    // PID 1 초기화: mount /proc, /sys, /dev
    // PL011 시리얼 디바이스 열기 (/dev/ttyAMA0)
    // "READY\n" 출력
    loop {
        let line = read_line(serial);
        let cmd: Command = serde_json::from_str(&line);
        let output = std::process::Command::new(&cmd.args[0])
            .args(&cmd.args[1..])
            .output();
        let resp = Response { status, stdout, stderr };
        write_line(serial, &serde_json::to_string(&resp));
    }
}
```

PID 1로 실행되므로 mount(/proc, /sys, /dev) 등 최소 초기화 포함.

## Pool

HVF 제약(프로세스당 VM 1개)으로 동시 VM 불가. 순차 재사용 모델:

```rust
pub struct SandboxPool {
    snapshot: OnceLock<VmSnapshot>,
    snapshot_mem: OnceLock<MmapedFile>,
}

impl SandboxPool {
    /// 첫 호출 시 스냅샷 자동 생성. 이후 호출은 캐시에서 CoW fork.
    /// HVF에서는 한 번에 하나의 MicroVm만 존재 가능.
    /// MicroVm이 Drop되면 VM이 정리되고, 다음 acquire()가 가능.
    pub fn acquire(&self) -> Result<MicroVm> {
        let snapshot = self.snapshot.get_or_try_init(|| {
            self.create_and_cache_snapshot()
        })?;
        let mem = self.snapshot_mem.get_or_try_init(|| {
            MmapedFile::open(&snapshot_mem_path())
        })?;
        MicroVm::fork_from(snapshot, mem)
    }
}
```

추후 KVM 백엔드에서는 동시 VM 가능 → pool_size > 1로 확장.

## opengoose-rig 통합

`SandboxedClient`가 기존 `McpClientTrait`를 감싸서 도구 호출을 VM 안에서 실행:

```rust
pub struct SandboxedClient<C: McpClientTrait> {
    inner: C,
    pool: Arc<SandboxPool>,
}

impl<C: McpClientTrait> McpClientTrait for SandboxedClient<C> {
    async fn call_tool(&self, name: &str, args: Value) -> Result<Value> {
        if self.should_sandbox(name) {
            let vm = self.pool.acquire()?;
            vm.exec(name, args).await
        } else {
            self.inner.call_tool(name, args).await
        }
    }
}
```

## Dependencies

```toml
[dependencies]
libc = "0.2"
linux-loader = "0.13"
vm-memory = { version = "0.18", features = ["backend-mmap"] }
vm-fdt = "0.3"              # DTB 생성
serde = { version = "1", features = ["derive"] }
bincode = "1"
log = "0.4"
thiserror = "2"

[target.'cfg(target_os = "macos")'.dependencies]
# HVF: extern "C" + framework linking via build.rs

[target.'cfg(target_os = "linux")'.dependencies]
kvm-ioctls = "0.19"          # 추후 Linux 지원 시
kvm-bindings = { version = "0.10", features = ["fam-wrappers"] }
```

libkrunfw: 빌드 의존성으로 사전 빌드된 커널 바이너리 연결.

## Testing

- HVF 제약: `serial_test` crate으로 VM 테스트 직렬화
- 단위 테스트: 스냅샷 직렬화, UART 에뮬레이션, DTB 생성
- 통합 테스트: VM 부팅 → 스냅샷 → fork → exec("echo hello") → 결과 검증

## Risks

- **libkrunfw 커널 호환성**: libkrunfw 커널이 우리 DTB/디바이스 모델과 맞지 않을 수 있음. 맞지 않으면 커스텀 최소 커널 빌드가 필요
- **HVF GIC API**: `hv_gic_create()`는 macOS 15+ 필요. 이전 버전 미지원
- **CoW fork 후 GIC 상태**: 스냅샷에서 GIC 상태 복원이 HVF API로 가능한지 검증 필요

## Out of Scope

- Linux KVM 백엔드 (trait만 정의, 구현은 추후)
- x86_64 아키텍처
- 네트워크 (virtio-net)
- 호스트 파일 공유 (virtio-fs)
- GPU 패스스루
- 동시 VM 풀 (HVF 제약, KVM 전환 시 추가)
