# 架构风险分析与修复方案（ARCHITECTURE RISK FIXES）

> **结论速览**
>
> - **当前最高优先 = P1-A**（STUB 创建函数返回垃圾指针，实际危害最大、改动最小且无依赖）。
> - 其次 **P0 三件套**：P0-C（删回退分支）→ P0-B（ID 全局分配）→ P0-A（ctor 惰性化，**必须最后做**）。
> - P1-A/P0 全部完成后，再做 P1-B（FAIL_FAST + FAKE 对齐）等行为配置类修复。
> - 快速获胜批（P0-B / P0-C / P1-C / P2-B / P2-C / P3-B）零依赖可并行，先落地即可消除大部分静默失败面。

---

## 一、概述与原则

**项目定位**：FluorateGL 是一个 Rust **edition 2024** 的 `cdylib` crate，通过 `LD_PRELOAD` 注入的 OpenGL→GLES 翻译层。架构分四层：`src/state` 状态层（ID 映射、线程局部 GL 状态）、`src/egl` + `src/gl` 导出拦截层（`#[unsafe(no_mangle)]` 导出 C ABI 函数）、`src/backend` dispatch 加载层（dlopen + dlsym 构建函数表）、`src/shader_translator` 翻译管线（GLSL→SPIR-V→GLSL ES，纯 CPU）。

**文档目的**：列出当前架构中的已知风险，给出可执行的修复方案、优先级排序、分期计划与验证方法。本文档不是设计文档，而是**风险驱动的修复任务书**：每项修复都给出目标、步骤、修改详情、风险、收益与验收验证。

**核心原则（加粗）**：
- **本项目注释部分有误，一切以实际代码逻辑为准**——本章第二节逐条列出"注释声称 vs 实际代码"的差异清单，作为后续修复的起点。
- **修复涉及注释与代码冲突时，先改代码再同步注释**，杜绝"注释领先于实现"。
- 优先级排序模型：**优先级 = 发生概率 × 危害程度 ÷ 修复成本**（详见第三节）。

**版本基线说明**：**本方案基于 Rust edition 2024 / 依赖版本以 Cargo.lock 为准**。所有涉及 Rust 特性或 crate API 的建议均已对照 `Cargo.toml` 与 `Cargo.lock` 核实（核实结果见文末"基线信息"），关键结论：
- `lru` crate **已在依赖中**（`Cargo.toml:14` 声明 `lru = "0.12"`，`Cargo.lock` 锁定 **0.12.5**，`src/shader_translator/cache.rs:6` 已实际使用 `LruCache`）——P2-D 直接复用，无需新增依赖；
- `OnceLock` / `AtomicU32` / `const _: () = assert!(...)` 均为 std 自带或 edition 2024 稳定特性（`src/config.rs:99-105` 已有编译期断言先例），无依赖冲突；
- `ctor` 实际解析版本 **1.0.11**，`#[ctor(unsafe)]` 语法已在 `src/init.rs:20` 使用，P0-A 不涉及更换。

**阅读约定**：文中所有 `文件:行号` 均指当前代码基线（**核实日期 2026-08-03**），防行号漂移；后续改动代码后如有行号变动，以实际代码为准。

---

## 二、注释 vs 实际代码差异清单

| # | 位置 | 注释声称 | 实际代码 | 处置 |
|---|---|---|---|---|
| 1 | `state/mod.rs:2-4` | 桌面 ID 由本库分配且**单调递增不复用** | `IdMap.next_id`（`id_map.rs:4`）存于 thread_local `STATE`（`state/mod.rs:105-107`），**每线程从 1 独立分配**，多线程共享场景 ID 必然碰撞 | 改注释 + **修复 P0-B** |
| 2 | `egl/exports.rs:52` | `config.rs:81-86` 声称版本"集中定义避免散落" | `EGL_VERSION` 硬编码 `b"1.4 FluorateGL\0"`（`egl/exports.rs:52`），**未引用** `REPORTED_EGL_MAJOR/MINOR`（`config.rs:115-117`） | **修复 P2-B** |
| 3 | `gl/exports.rs:424` | 声明 `GL_ARB_multi_draw_indirect`（桌面对应扩展名） | `capabilities.rs:106-107` 实际检测 `GL_EXT_multi_draw_indirect`（GLES 侧扩展名），ARB/EXT 名称不一致 | **修复 P1-B**（校验剔除） |
| 4 | `egl/exports.rs:273-275` | —（无注释，常量引用不合理） | `EGL_CONTEXT_CLIENT_VERSION` 查询复用 `REPORTED_GL_MAJOR`（=3），GL/EGL 语义常量混用，值**巧合正确**（GLES3 基线即 3） | **修复 P2-B** |
| 5 | `egl/exports.rs:194-237` | —（无注释） | `eglCreateContext` 属性改写循环（209-232）仅靠 `EGL_NONE` 终止，**无上限边界**，宿主传非法数组时越界读 | **修复 P1-C** |
| 6 | `backend/dispatch.rs:342-351` | —（无注释） | `all_stub` 按 `size_of::<Self>() / size_of::<unsafe extern "C" fn()>()` 逐槽填充（347-349），**硬依赖全指针布局** | **修复 P2-A** |

> 备注项（过时注释，顺手改）：`gl/shader.rs:299` 注释声称"GLES 创建失败（gles_id=0 被 alloc）"，与 `shader.rs:55-59` 实际代码矛盾——gles_id==0 时**直接返回 0，不 alloc**。该注释是历史实现残留，随 P0-B 一并修正（不影响 ID 分配逻辑本身）。

---

## 三、风险清单与优先级排序

**排序模型**：优先级 = 发生概率 × 危害程度 ÷ 修复成本。概率/危害/成本均为相对评级（高/中/低/极低），成本含上下文切换与回归面。

| 编号 | 风险 | 概率 | 危害 | 成本 | 等级 |
|---|---|---|---|---|---|
| R1 | ctor 全量初始化 + dlopen（`init.rs:20-26`） | 高 | 高 | 中高 | **P0-A** |
| R2 | 桌面 ID 跨线程碰撞 | 中 | 高 | 极低 | **P0-B** |
| R3 | 回退 dlopen 二次加载 + ctor 重入（`init.rs:41-46`） | 低 | 高 | 极低 | **P0-C** |
| R4 | STUB 模式创建函数返回非 null 垃圾指针（`egl_sys/dispatch.rs:51-60`，AArch64 x0 残留） | 高 | 高 | 低 | **P1-A**（实际最高优先） |
| R5 | GLES 加载失败静默降级 | 中 | 高 | 中 | **P1-B** |
| R6 | FAKE_EXTENSIONS 57 项与 capabilities 无自动对齐 | 中 | 中 | 中 | **P1-B** |
| R7 | 属性循环越界读 | 低 | 中 | 极低 | **P1-C** |
| R8 | all_stub 布局脆弱 | 低 | 低 | 极低 | **P2-A** |
| R9 | EGL 常量散落 / 语义混用 | 中 | 低 | 极低 | **P2-B** |
| R10 | glGetStringi 越界返回 null | 低 | 低 | 1 行 | **P2-C** |
| R11 | thread_local 翻译缓存无上限 | 低 | 低 | 低 | **P2-D**（可降 P3） |
| R12 | eglMakeCurrent `log::info!` ×2 | 高 | 极低 | 极低 | **P3-B** |
| R13 | 无导出符号表 | — | — | 中 | **P3-A** |

**触发场景（一句话）**：
- R1：任何进程注入（包括只需翻译管线的纯 CPU 工具）都在 ctor 阶段立即 dlopen EGL/GLES，与宿主自身初始化竞争，且注入零开销优势尽失。
- R2：异步线程 `glCreateShader` 等创建对象、渲染线程查询/删除 → 两个线程各自从 1 起分配，ID 错配，`translate_binding_to_desktop` 查不到映射降级为 miss（当前不崩溃，但功能错乱）。
- R3：旧 Android 不支持 `RTLD_NOLOAD`（`init.rs:42-46`）→ 普通 dlopen 重新加载自身 → ctor 重入。
- R4：EGL/GLES 库缺失或加载失败 → `with_egl_dispatch` 兜底 `all_stub`（`backend/mod.rs:173-176`）→ AArch64 上 `eglCreateContext` 返回 x0 保留的调用前值（非 null 垃圾指针）→ 宿主判定创建成功 → 黑屏挂起难排查。
- R5：真实驱动缺失/损坏时仅 `log::warn!`（`init.rs:102-112`），默认静默降级，线上黑屏无法归因。
- R6：扩展声明与能力检测不一致（差异 #3 的 ARB/EXT 改名）→ 宿主查询后调用 stub 函数崩溃或行为降级。
- R7：宿主传无 `EGL_NONE` 终止的属性数组 → `eglCreateContext` 循环越界读。
- R8：dispatch 结构体混入非函数指针字段或 ABI 变化 → 逐槽填充错位（现全指针布局下低概率）。
- R9：改 `REPORTED_GL_MAJOR` 或 EGL 版本字符串时遗漏同步 → 版本报告不一致（GL 侧有断言 `config.rs:99-105`，EGL 侧无）。
- R10：宿主不判 null 直接解引用 `glGetStringi` 越界返回值 → 崩溃。
- R11：大量唯一 shader 源码（mod 热重载/动态生成）→ thread_local 缓存无上限增长。
- R12：每帧切上下文时 `eglMakeCurrent` 打两条 info 日志 → 日志刷屏与性能噪音。
- R13：`eglGetProcAddress` 依赖 `dlsym(self_handle)` 自查（`init.rs:16-18`），无结构化符号清单，自查与 FAKE_EXTENSIONS 校验都缺基础设施。

---

## 四、修复方案

### 4.1 P0 类修复（高风险）

#### 4.1.1 P0-A ctor 惰性化

**优先级：P0-A（高风险）｜ 预估工期：1~2 天 ｜ 依赖：P0-C 先行**（删回退分支后，`capture_self_handle` 不再存在二次 dlopen 风险，惰性化才能安全搬移）

- 🎯 **目标**：注入后不立即 dlopen；首次 GL/EGL 调用时才初始化后端。消除注入开销、消除与宿主初始化阶段的竞争。
- 📋 **步骤**：
  1. `#[ctor]` 函数体（`src/init.rs:20-26`）缩减为 `Config::from_env()` + `set_config`，不再执行后端加载；
  2. 新增 `ensure_backend_initialized()`（`OnceLock` 包裹，std 自带），把 `capture_self_handle` + `init_egl` + `init_gles` 从 `fluorategl_init`（`src/init.rs:79-118`）拆分搬入；
  3. 触发点加守卫：`with_egl_dispatch` / `with_gles_dispatch`（`src/backend/mod.rs:61-78、164-178`）、`eglGetProcAddress`、`get_self_handle`（`src/init.rs:68-70`）入口调用 `ensure_backend_initialized()`；
  4. 保留 `FLUORATEGL_SKIP_BACKEND=1` 跳过语义（`init.rs:95-97` 不变）。
- 🔧 **修改详情**：`src/init.rs:20-26`（ctor 函数体）、`src/init.rs:79-118`（`fluorategl_init` 拆分出 `ensure_backend_initialized`）、`src/egl/exports.rs` + `src/gl/exports.rs`（各导出函数入口守卫，重点是 `eglGetProcAddress` 与首批 GL/EGL 入口）。
- ⚠️ **风险**：① OnceLock 首次初始化失败后永久失败，重试语义需与 P1-B 的 FAIL_FAST 交互（明确：失败即保持未初始化，后续调用走 stub 兜底，与现状一致）；② 并发首调用由 OnceLock 保证只执行一次，无竞态；③ 部分应用期望 ctor 期间 GL 即可用（回归风险，需真机首帧验证）。
- 📈 **收益**：注入零开销；消除与宿主初始化竞争；为 P1-B 的 FAIL_FAST 提供干净失败点（失败发生在首次调用而非注入瞬间，日志可归因）。
- ✅ **验证**：§六层次 3（注入冒烟观察 ctor 阶段无 dlopen）+ 层次 4（真机首帧回归，重点 P0-A 首帧）。

#### 4.1.2 P0-B 桌面 ID 全局分配

**优先级：P0-B（高风险）｜ 预估工期：约 10 行，成本极低 ｜ 依赖：无**

- 🎯 **目标**：桌面 ID 全局唯一（跨线程），杜绝 R2 的跨线程碰撞错配。
- 📋 **步骤**：
  1. 删除 `IdMap.next_id` 字段（`src/state/id_map.rs:3-39`），改为模块级 `static NEXT_ID: AtomicU32`（std 自带，edition 2024 无版本问题）；
  2. `alloc`（`id_map.rs:18-24`）用 `NEXT_ID.fetch_add(1, Ordering::Relaxed)` 取号；
  3. 同步修 `state/mod.rs:2-4` 注释（差异 #1：改为"桌面 ID 全局唯一单调分配，GLES ID 由驱动分配"）。
- 🔧 **修改详情**：`src/state/id_map.rs`、`src/state/mod.rs:2-4`。
- ⚠️ **风险**：`fetch_add` 理论上 u32 回绕（需 42.9 亿次分配），概率可忽略；可加 `debug_assert` 兜底。锁语义：fetch_add 为原子操作，无锁竞争问题。
- 📈 **收益**：跨线程对象共享不再错配；为 `glGetShaderiv` 等跨线程查询场景消除假阴性（`shader.rs:296-302` 的 miss 路径不再因 ID 碰撞误触发）。
- ✅ **验证**：§六层次 2 单测（多线程并发 alloc 唯一性测试）。

#### 4.1.3 P0-C 删除 capture_self_handle 回退

**优先级：P0-C（高风险）｜ 预估工期：成本极低 ｜ 依赖：无**

- 🎯 **目标**：消除 R3——`RTLD_NOLOAD` 失败后二次 dlopen 重新加载自身导致 ctor 重入。
- 📋 **步骤**：
  1. `src/init.rs:41-46` 删除回退分支：`RTLD_NOLOAD` 失败即放弃，`SELF_HANDLE = None` + `log::warn!`（保留原 warn 路径 `init.rs:55-58` 的提示语义）；
  2. 注释注明长期方向：自建导出符号表替代 `dlsym(self_handle)`，与 P3-A 衔接（见 4.4.1）。
- 🔧 **修改详情**：`src/init.rs:41-46`。
- ⚠️ **风险**：个别宿主 dlopen 后 dlsym 定位变化（可接受——`eglGetProcAddress` 自查失败时降级走 `RTLD_DEFAULT` 兜底，见 §五 ②）。
- 📈 **收益**：彻底移除 ctor 重入路径；为 P0-A 惰性化铺路（P0-A 依赖本修复先行）。
- ✅ **验证**：§六层次 3（`LD_DEBUG=libs` 观察注入阶段无二次 dlopen 本库记录）。

### 4.2 P1 类修复（中风险）

#### 4.2.1 P1-A STUB 创建函数安全值（实际最高优先）

**优先级：P1-A（中风险，实际最高优先）｜ 预估工期：约 20 行 ｜ 依赖：无**

> **背景**：`EglDispatch::all_stub`（`egl_sys/dispatch.rs:51-60`）将所有槽填成 `void` stub（`unsafe extern "C" fn stub_fn() {}`）。AArch64 上调用约定下，`x0` 保留调用前的寄存器值 → `eglCreateContext` 等**返回指针的函数返回非 null 垃圾指针** → 宿主判定创建成功 → 后续一切调用落到 void stub → 黑屏挂起，且无任何错误日志。这是当前**实际危害最高**的风险（R4：概率高 × 危害高 ÷ 成本低）。

- 🎯 **目标**：STUB 模式下创建类函数返回 `null`、查询类返回安全值（0/空串/TRUE），宿主走失败错误路径而非静默黑屏。
- 📋 **步骤**：
  1. **方案 (a)（推荐，为主）**：`egl_sys/dispatch.rs` 的 `all_stub` 按**签名类别**填对应 stub：`stub_void`（`()`）、`stub_zero_u32`（返回 0）、`stub_null_ptr`（返回 null）、`stub_true`（返回 `EGL_TRUE=1`）；
  2. **方案 (b)（兜底）**：`egl/exports.rs` 创建类函数（`eglCreateContext` / `eglGetDisplay` / `create_*_surface` 等）加 `is_stub` 守卫（比较槽指针与 stub 地址，机制同 `backend/dispatch.rs:410` 的 `stub` 字段 + `backend/mod.rs:132` 的比对先例），STUB 时返回 null + `log::error!`；
  3. 推荐 (a) 为主 (b) 兜底，两层都做。
- 🔧 **签名清单（需安全值的完整清单，与 `egl_sys/dispatch.rs:5-47` 字段逐一核对）**：
  - **创建/获取类（返回 `*mut c_void` → stub 返回 null）**：`get_display`、`create_window_surface`、`create_pbuffer_surface`、`create_pbuffer_from_client_buffer`、`create_pixmap_surface`、`create_context`、`get_current_context`、`get_current_surface`、`get_current_display`、`get_proc_address`；
  - **状态/查询类（返回 `u32` → stub 返回 0）**：`initialize`、`terminate`、`get_configs`、`choose_config`、`get_config_attrib`、`destroy_surface`、`surface_attrib`、`bind_tex_image`、`release_tex_image`、`destroy_context`、`make_current`、`wait_client`、`wait_native`、`wait_gl`、`release_thread`、`query_api`、`swap_buffers`、`swap_interval`、`copy_buffers`、`get_error`；
  - **字符串类（返回 `*const c_char` → stub 返回 `b"\0"`）**：`query_string`；
  - **布尔类（返回 `u32` 且语义为布尔 → stub 返回 1）**：`bind_api`。
- ⚠️ **风险**：签名分类必须与字段类型逐一核对（`transmute` 场景，误配即 UB）；方案 (a) 改动影响所有 EGL stub 路径，需注入冒烟回归；`bind_api` 语义上返回 `EGL_TRUE` 更安全（`exports.rs:182-184` 已强制转发 `EGL_OPENGL_ES_API`）。
- 📈 **收益**：黑屏挂起 → 宿主可见失败路径；与 P0-A 互为补充（惰性化后 STUB 出现概率更低，但 STUB 仍是加载失败时的最终兜底，必须安全）。
- ✅ **验证**：§六层次 2（STUB 签名映射纯函数测试）+ 层次 3（STUB 模式下 `eglCreateContext` 返回 null 可观测）。

> 备注项（已完成）：**GLES 侧 `EglDispatch::all_stub`（`egl_sys/dispatch.rs:59-126`）已按签名类别填安全值**——`stub_null_ptr`（返回指针类）/ `stub_zero_u32`（u32 状态类）/ `stub_true`（EGLBoolean）/ `stub_empty_string`（字符串类），并有签名映射单测（`dispatch.rs:213-238`），方案 (a) 落地完成；方案 (b) 的 STUB 守卫亦已就位（`eglCreateContext` 入口 `exports.rs:245-248`，STUB 模式直接返回 null）。

#### 4.2.2 P1-B FAIL_FAST + FAKE_EXTENSIONS 对齐

**优先级：P1-B（中风险）｜ 预估工期：中 ｜ 依赖：阶段 1 的 P2-B 常量化先行**（FAKE 校验依赖常量统一，避免边改边乱）

**FAIL_FAST 部分**：
- 🎯 **目标**：GLES 加载失败时提供显式失败信号（opt-in），黑屏可归因。
- 📋 **步骤**：
  1. `Config`（`src/config.rs:25-31`）增加 `fail_fast: bool`（读 `FLUORATEGL_FAIL_FAST=1`，`from_env` 于 `config.rs:34-60` 解析）；
  2. `init_egl` / `init_gles` 失败时（`src/init.rs:102-112`），若 `fail_fast` 开启则 `std::process::abort()`，**abort 前日志最后一条指明原因**（EGL/GLES 哪个库、什么错误）；
  3. **默认关闭，行为不变**——点明 `init.rs:98-101` 注释：翻译管线（GLSL→SPIR-V→GLSL ES）是纯 CPU 操作，不依赖 EGL/GLES，"纯翻译场景"（fork worker、离线测试）依赖无后端也能跑。
- 🔧 **修改详情**：`src/config.rs`、`src/init.rs:102-112`。

**FAKE 对齐部分**：
- 🎯 **目标**：`FAKE_EXTENSIONS` 声明与 `GlesCapabilities` 检测结果自动对齐，消除差异 #3 与 R6。
- 📋 **步骤**：
  1. `FAKE_EXTENSIONS`（`gl/exports.rs:409-475`，57 项）从 `static &[&[u8]]` 改为 `OnceLock<Vec<&[u8]>>`（惰性构建，首次 `glGetString(GL_EXTENSIONS)` 时组装）；
  2. **行为依赖型扩展**（draw / base_vertex / indirect / buffer_storage 系列：`GL_ARB_draw_indirect`、`GL_ARB_multi_draw_indirect`/`GL_EXT_multi_draw_indirect`、`GL_ARB_draw_elements_base_vertex`/`GL_OES_draw_elements_base_vertex`、`GL_ARB_base_instance`/`GL_EXT_base_instance`、`GL_EXT_multi_draw_elements_base_vertex`、`GL_ARB_buffer_storage`）与 `GlesCapabilities`（`capabilities.rs:95-114`）**显式映射校验**：声明了但 caps=false → `log::warn!` + 从列表剔除；
  3. 自动修正差异 #3：`gl/exports.rs:424` 的 `GL_ARB_multi_draw_indirect` 与 `capabilities.rs:106-107` 检测的 `GL_EXT_multi_draw_indirect` 名称不一致 → 按 caps 结果二选一（或仅声明 `GL_EXT_multi_draw_indirect`，宿主大多同时接受）。
- 🔧 **修改详情**：`src/gl/exports.rs:409-475`（含 424 行注释同步）、`src/backend/capabilities.rs`（如需暴露单一判断入口）。
- ⚠️ **风险**：FAKE 剔除可能改变宿主渲染路径选择（声明少了 → 宿主走低配路径）——真机回归必测；`abort()` 在 init 阶段调用，必须保证日志已落盘（logcat 直出，无缓冲问题）。
- 📈 **收益**：黑屏可归因（FAIL_FAST）；声明与实现一致（FAKE），宿主不再因"声明了但函数是 stub"而崩溃。
- ✅ **验证**：§六层次 3（`FLUORATEGL_FAIL_FAST=1` 时 abort 且日志有原因）+ 层次 4（FAKE 剔除后宿主行为回归，重点 Sodium）。

#### 4.2.3 P1-C eglCreateContext 属性循环上限

**优先级：P1-C（中风险）｜ 预估工期：成本极低 ｜ 依赖：无**

- 🎯 **目标**：消除 R7——属性改写循环（`egl/exports.rs:209-232`）越界读。
- 📋 **步骤**：循环（`209` 行 `loop`）加**128 对上限**（即 `i < 256`）：超过上限说明属性数组非法（无 `EGL_NONE` 终止），返回 `EGL_NO_CONTEXT`（null 指针）+ `log::warn!`，不触碰越界内存。
- 🔧 **修改详情**：`src/egl/exports.rs:194-237`（循环体 209-232）。
- ⚠️ **偏离说明（实现已定稿）**：超限时**返回 `EGL_NO_CONTEXT`（null 指针），而非 `EGL_BAD_ATTRIBUTE`**——`eglCreateContext` 返回类型是 `*mut c_void`，若返回 `EGL_BAD_ATTRIBUTE`（0x3054）数值，宿主会把它当作**有效 context 指针**继续使用，形成垃圾指针漏洞，与 P1-A 的目标（失败路径不得产生伪指针）直接冲突。null 指针让宿主走标准"创建失败"错误路径（`eglGetError` 仍可查错）。实现见 `src/egl/exports.rs:261-266`，注释同步记录此理由。
- ⚠️ **风险**：无实质风险；上限 128 对远大于真实使用（宿主常用 <20 对），不会误伤。
- 📈 **收益**：防御宿主导入损坏/构造异常的属性列表，杜绝越界读。
- ✅ **验证**：§六层次 2 单测（构造超长无终止属性数组，断言返回 `EGL_NO_CONTEXT`（null））。

### 4.3 P2 类修复（低风险）

#### 4.3.1 P2-A all_stub 编译期约束

**优先级：P2-A（低风险）｜ 预估工期：约 4 行 ｜ 依赖：无**

- 🎯 **目标**：all_stub 逐槽填充（差异 #6）的"全指针布局"假设在**编译期**固化，未来结构体改动立即暴露。
- 📋 **步骤**：`backend/dispatch.rs`（342-351）与 `egl_sys/dispatch.rs`（51-60）两个 dispatch struct 各加：
  - `#[repr(C)]`（布局确定化，消除重排风险）；
  - `const _: () = assert!(size_of::<Self>() % size_of::<unsafe extern "C" fn()>() == 0);`（size 整除性编译期断言，edition 2024 稳定，`config.rs:99-105` 已有同款先例）。
- 🔧 **修改详情**：`src/backend/dispatch.rs`、`src/egl_sys/dispatch.rs`。
- ⚠️ **风险**：无版本冲突（const 断言为语言特性）；若未来混入非指针字段，断言直接编译失败——这正是目的（提示改为显式初始化而非逐槽填充）。
- 📈 **收益**：布局脆弱性（R8）从运行时隐患变为编译期错误。
- ✅ **验证**：§六层次 1（能编译即通过）。

#### 4.3.2 P2-B EGL 常量同步

**优先级：P2-B（低风险）｜ 预估工期：成本极低 ｜ 依赖：无**

- 🎯 **目标**：修差异 #2、#4——EGL 版本字符串与 `REPORTED_EGL_*` 同步、CLIENT_VERSION 与 GL 语义常量解耦。
- 📋 **步骤**：
  1. `egl/exports.rs:52` 的 `b"1.4 FluorateGL\0"` 改为引用 `REPORTED_EGL_MAJOR/MINOR`（`config.rs:115-117`）生成，并加**编译期断言**（仿 `config.rs:99-105`：EGL 版本字符串首字符与 MAJOR/MINOR 一致）；
  2. 新增 `REPORTED_EGL_CLIENT_VERSION: i32 = 3` 常量（放 `config.rs` 版本信息区，81-117），`egl/exports.rs:273-275` 的 `EGL_CONTEXT_CLIENT_VERSION` 查询改用该常量，与 `REPORTED_GL_MAJOR` 解耦（GL/EGL 语义分离，杜绝"值巧合正确"）。
- 🔧 **修改详情**：`src/config.rs`（新增常量 + 断言）、`src/egl/exports.rs:52、273-275`。
- ⚠️ **风险**：无依赖冲突；注意 `REPORTED_EGL_CLIENT_VERSION` 与 GLES3 基线（§五 ⑤）保持一致，改动时同步。
- 📈 **收益**：EGL 侧版本报告与 GL 侧同等受约束，杜绝散落不一致（R9）。
- ✅ **验证**：§六层次 1（编译期断言通过）。

#### 4.3.3 P2-C glGetStringi 越界空串

**优先级：P2-C（低风险）｜ 预估工期：1 行 ｜ 依赖：无**

- 🎯 **目标**：`glGetStringi` 越界返回空串而非 null（R10），防宿主不判 null 直接解引用崩溃。
- 📋 **步骤**：`gl/exports.rs:575-580` 的越界分支（579 行 `std::ptr::null()`）改为返回 `b"\0"` 的指针（与 `glGetString` 兜底风格一致，见 389-400 行）。
- 🔧 **修改详情**：`src/gl/exports.rs:575-580`。
- ⚠️ **风险**：无；返回空串符合 GL 规范允许的错误处理（调用方读 GL 错误码）。
- 📈 **收益**：一行消除潜在崩溃点。
- ✅ **验证**：§六层次 2 单测（index 越界 → 返回非 null 空串）。

#### 4.3.4 P2-D thread_local 缓存 LRU 上限

**优先级：P2-D（低风险，收益小，可降 P3）｜ 预估工期：低 ｜ 依赖：无（lru 已引入）**

- 🎯 **目标**：`thread_local` 翻译缓存（`gl/shader.rs:139-143` 的 `state.shader_translation_cache`，FxHashMap）无上限（R11），限制其增长或删除该层。
- 📋 **步骤**：
  1. **方案 (a)**：给 `shader_translation_cache`（`state/mod.rs:58`）加 **32 条 LRU 上限**；
  2. **方案 (b)**：直接**删除该 thread_local 层**——全局缓存已够（`shader_translator/cache.rs:115` 是全局 `OnceLock<ShaderCache>`，`119` 行 `new(64)`，64 条默认容量足够覆盖 MC 全部 shader）。
  3. 若走方案 (a)：**lru crate 版本核对（见硬性要求）**——`Cargo.toml:14` 已声明 `lru = "0.12"`，`Cargo.lock` 锁定 **0.12.5**，且 `cache.rs:6` 已实际使用；本方案直接复用 `lru::LruCache`，**无需新增依赖、无版本冲突**。注意 lru 0.12 API：`LruCache::new` 需 `NonZeroUsize`（`cache.rs:27-28` 已有用法先例）。
- 🔧 **修改详情**：`src/state/mod.rs:58`（或删除该字段 + `shader.rs:139-193` 对应分支）。
- ⚠️ **风险**：FxHashMap 换 LruCache 后热路径多一次容量检查（可忽略）；若删除 thread_local 层，需确认全局缓存命中率（重载场景 key 相同可命中）。
- 📈 **收益**：内存增长有界；方案 (b) 还简化 State 结构（少一个字段）。
- ✅ **验证**：§六层次 2（如有缓存行为单测）+ 性能对比（帧率无回退）。

### 4.4 P3 类修复（评估/低优先）

#### 4.4.1 P3-A 导出符号表

**优先级：P3-A（评估/低优先）｜ 预估工期：中 ｜ 依赖：阶段 3 后启动**

- 🎯 **目标**：建立本库 `#[no_mangle]` 导出符号的结构化清单，服务 `eglGetProcAddress` 自查与 FAKE_EXTENSIONS 校验（为 P1-B 提供基础设施）。
- 📋 **步骤**：
  1. **明确不建议** dispatch 全量生成化（改动面过大、收益不确定，P1-A 已用更小成本解决 STUB 安全问题）；
  2. 用宏收集 `#[no_mangle]` 导出（`egl/exports.rs` + `gl/exports.rs` 各函数）→ 生成 `static SYMBOLS` 符号表；
  3. 符号表服务：`eglGetProcAddress` 自查（替代/补充 `dlsym(self_handle)`，`init.rs:16-18`）、FAKE_EXTENSIONS 声明校验（声明项必须能在 SYMBOLS 中找到）。
- 🔧 **修改详情**：新增符号收集宏（建议放 `src/egl_sys` 或独立 `src/symbols` 模块）。
- ⚠️ **风险**：宏收集需要统一导出注解约定；与 P0-C 的长期方向（自建符号表替代 dlsym）衔接，P0-C 先落地回退删除、此处再补基础设施。
- 📈 **收益**：`eglGetProcAddress` 不再依赖 dlopen 自身句柄；FAKE 校验有权威数据源。
- ✅ **验证**：§六层次 3/4（`eglGetProcAddress` 返回本库函数指针的正确性）。

#### 4.4.2 P3-B 日志清理

**优先级：P3-B（评估/低优先）｜ 预估工期：成本极低 ｜ 依赖：无**

- 🎯 **目标**：消除 R12——`eglMakeCurrent` 的 `log::info!` ×2 刷屏（`egl/exports.rs:253`、`261`）。
- 📋 **步骤**：两处 `log::info!` 降为 `log::debug!`（保留日志以支持 `FLUORATEGL_LOG=debug` 下的上下文切换诊断）。
- 🔧 **修改详情**：`src/egl/exports.rs:253、261`。
- ⚠️ **风险**：无。
- 📈 **收益**：info 级别日志量显著下降（每帧多次 eglMakeCurrent 场景）。
- ✅ **验证**：§六层次 3 冒烟（观察 info 级别无 eglMakeCurrent 噪音）。

---

## 五、接受 + 文档化清单

以下行为**有意为之**，不做代码修改，仅文档化（现状描述 + 接受理由 + 文档位置），防止后续被误判为 bug 而"修复"。

| # | 行为 | 现状 | 为什么接受 | 文档位置 |
|---|---|---|---|---|
| ① | 跨线程对象共享错配 | ID 查不到映射 → 降级为 miss（`gl/exports.rs:526-535` 的 `warn_binding_id_miss`，`gl/shader.rs:296-302` 的 `warn_shader_id_miss`） | 只要不崩溃即可接受（GL 语义本不保证跨上下文共享对象）；P0-B 降低触发概率，miss 路径作为最终兜底保留 | `state/mod.rs:2-4` 注释更新 + 本清单 |
| ② | RTLD_DEFAULT 兜底 | `eglGetProcAddress` 自查失败时降级走 `RTLD_DEFAULT`（`egl/exports.rs` 的 `eglGetProcAddress` 路径） | 兼容个别宿主 dlopen 后 dlsym 定位变化（P0-C 后自查失败仅影响 proc address 兜底精度）；P3-A 长期替代 | `src/init.rs:16-18` 注释 + 本清单 |
| ③ | 非行为依赖型扩展的静态声明 | `FAKE_EXTENSIONS` 中仅"声明不保证实现"的项（纹理压缩、debug、ASTC 等） | 宿主只查询不调用或调用时另有兜底；行为依赖型（draw/base_vertex/indirect/buffer_storage）已由 P1-B 强制对齐，其余项静态声明可接受 | `gl/exports.rs:409-475` 注释（P1-B 改造时同步标注） |
| ④ | 默认静默降级 | GLES 加载失败默认仅 warn（`init.rs:102-112`） | 翻译管线纯 CPU 不依赖后端（`init.rs:98-101`），默认行为不能破坏纯翻译场景；FAIL_FAST 为 opt-in | `init.rs:98-101` 注释 + 本清单 |
| ⑤ | 强制 CLIENT_VERSION=3 | `eglCreateContext` 属性改写强制 `EGL_CONTEXT_CLIENT_VERSION=3`（`egl/exports.rs:217-220`） | GLES3 是项目基线（capabilities.rs:88-93 要求 3.1+），伪装桌面 3.3 必须 GLES3 | `egl/exports.rs:217-220` 注释 + 本清单 |

---

## 六、验证方法

| 层次 | 手段 | 覆盖对象 |
|---|---|---|
| 1 编译期 | `cargo build` + `cargo clippy`；编译期断言类修复（P2-A / P2-B）**直接看能否编译** | P2-A、P2-B、全部回归 |
| 2 单元/离线测试 | `cargo test`；STUB 签名映射纯函数测试、属性超长数组单测、桌面 ID 多线程唯一性测试、glGetStringi 越界单测 | P0-B、P1-A、P1-C、P2-C |
| 3 注入冒烟 | `LD_PRELOAD` 注入**不依赖 GLES 后端**的测试程序：观察 ctor 阶段无 dlopen（`LD_DEBUG=libs` 或日志时间戳）、STUB 下 `eglCreateContext` 返回 null、`FAIL_FAST=1` 时 abort 且日志有原因 | P0-A、P0-C、P1-A、P1-B |
| 4 真机验证 | 注入真实 GL 应用（含多线程渲染），渲染正常、无黑屏、logcat 无 error；重点回归 P0-A 首帧、P1-B FAKE 剔除后宿主行为 | 全部，重点 P0-A / P1-B |

---

## 七、任务分期与依赖关系

```
阶段 1 快速获胜批（零依赖，可并行）
  P0-B  ID 全局分配          ──┐
  P0-C  删回退分支            ──┼── 验收：cargo build + cargo test 全绿（层次 1+2）
  P1-C  属性循环上限          ──┤
  P2-B  EGL 常量同步          ──┤（P1-B 依赖此先行）
  P2-C  glGetStringi 越界    ──┤
  P3-B  日志清理              ──┘
        │
阶段 2 dispatch 同区域批（同批文件：egl_sys/dispatch.rs + backend/dispatch.rs）
  P1-A  STUB 安全值（主） + P2-A 编译期约束
        └── 验收：层次 1 + 2 + 注入冒烟（层次 3）
        │
阶段 3 行为配置批
  P1-B  FAIL_FAST + FAKE 对齐（依赖阶段 1 的 P2-B 常量化）
        └── 验收：层次 3 + 真机回归（层次 4）
        │
阶段 4 架构级
  P0-A  ctor 惰性化（依赖 P0-C 先行 + P1-B 的失败点语义）
        └── 验收：层次 3 + 真机多线程回归（层次 4）

独立评估线：
  P3-A  导出符号表（阶段 3 后启动，为 P1-B/eglGetProcAddress 提供基础设施）
  P2-D  缓存上限（随时可做，收益小，可降级并入 P3）
```

**关键结论**：
- **P0-A 必须最后做**——它依赖 P0-C（回退删除）与 P1-B（失败点语义）先行，且架构改造回归面最大；
- **P1-A 建议提前**——实际危害最高（黑屏挂起），但改动小（约 20 行）、零依赖，可与阶段 1 并行启动。

---

## 八、附：修复-验证映射速查表

| 修复编号 | 改动文件 | 验证手段（§六层次） | 验收标志 |
|---|---|---|---|
| P0-A | `src/init.rs:20-26、79-118`、`src/backend/mod.rs:61-78、164-178`、`egl/exports.rs`、`gl/exports.rs` | 3 + 4 | 注入冒烟 ctor 阶段无 dlopen；真机首帧正常、无黑屏 |
| P0-B | `src/state/id_map.rs`、`src/state/mod.rs:2-4` | 2 | 多线程并发 alloc 唯一性单测通过；ID 映射注释已更新 |
| P0-C | `src/init.rs:41-46` | 3 | `LD_DEBUG=libs` 无二次 dlopen 本库；SELF_HANDLE 缺失时仅 warn |
| P1-A | `src/egl_sys/dispatch.rs:51-60`、`src/egl/exports.rs` | 2 + 3 | stub 签名映射单测通过；STUB 下 `eglCreateContext` 返回 null |
| P1-B | `src/config.rs`、`src/init.rs:102-112`、`src/gl/exports.rs:409-475` | 3 + 4 | `FAIL_FAST=1` abort 且日志有原因；FAKE 与 caps 对齐、ARB/EXT 差异 #3 消除 |
| P1-C | `src/egl/exports.rs:194-237` | 2 | 超长属性数组单测返回 `EGL_NO_CONTEXT`（null 指针） |
| P2-A | `src/backend/dispatch.rs:342-351`、`src/egl_sys/dispatch.rs:51-60` | 1 | `#[repr(C)]` + 断言编译通过 |
| P2-B | `src/config.rs`、`src/egl/exports.rs:52、273-275` | 1 | EGL 版本字符串断言编译通过；CLIENT_VERSION 使用 `REPORTED_EGL_CLIENT_VERSION` |
| P2-C | `src/gl/exports.rs:575-580` | 2 | 越界单测返回非 null 空串 |
| P2-D | `src/state/mod.rs:58`、`src/gl/shader.rs:139-193` | 2 + 性能对比 | 缓存有界（32 条 LRU 或删除该层）；帧率无回退 |
| P3-A | 新增符号收集宏（`src/egl_sys` 或 `src/symbols`） | 3/4 | `eglGetProcAddress` 自查返回本库函数指针 |
| P3-B | `src/egl/exports.rs:253、261` | 3 | info 级别无 eglMakeCurrent 噪音 |

---

## 基线信息

- **代码基线**：文档基于 git 当前工作树（本环境**非 git 仓库**，未做版本对比，行号以 2026-08-03 当日文件内容为准）。
- **代码核实日期**：**2026-08-03**（所有 `文件:行号` 引用均以此日核实为准）。
- **Rust 工具链/项目版本**：FluorateGL 0.2.0，`edition = "2024"`，`crate-type = ["cdylib", "rlib"]`（Cargo.toml:1-8）。
- **依赖版本核对结果**（`Cargo.toml` 声明 → `Cargo.lock` 实际锁定）：
  - `ctor`：声明 `1.0.9` → 实际 **1.0.11**（`#[ctor(unsafe)]` 语法可用，init.rs:20 已在用）；
  - `libc`：声明 `0.2.186` → 实际 **0.2.189**；
  - `log`：声明 `0.4.33` → 实际 **0.4.33**；
  - `lru`：声明 `0.12` → 实际 **0.12.5**（**已在依赖中**，`shader_translator/cache.rs:6` 使用中；P2-D 直接复用，无需新增依赖）；
  - `regex`：声明 `1` → 实际 **1.13.1**；
  - `rustc-hash`：声明 `2.1.3` → 实际 **2.1.3**；
  - `sha2`：声明 `0.10` → 实际 **0.10.9**；
  - `shaderc`：声明 `0.10.1`（`build-from-source`）→ 实际 **0.10.1**；
  - `spirv-cross2`：声明 `0.7.1`（glsl+cpp）→ 实际 **0.7.1**。
- **版本兼容性结论**：本文档所有 Rust 特性建议（`OnceLock`、`AtomicU32::fetch_add`、`const _: () = assert!` 编译期断言、`#[repr(C)]`）均为 std 自带或 edition 2024 稳定特性；crate API 建议（`lru::LruCache`，0.12 需 `NonZeroUsize`）与锁定版本 0.12.5 兼容。**未发现任何修复建议与当前依赖版本冲突**。
