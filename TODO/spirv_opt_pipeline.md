# SPIRV-Tools Optimizer Pass 链接入方案（FluorateGL）

**状态：已存档待确认（2026-08-08）**
**目标链**：shaderc (OpenGL 450 / Zero) → SPIRV-Tools Optimizer（标准 pass 链）→ spirv-cross → GLES 3.2/3.1
**约束**：fail-open 永不劣化；差分 hash 为最终裁决

## 1. 阶段划分

### 阶段 0：Spike 可行性验证（不碰主仓库，独立临时 crate）
位置 /data/data/com.termux/files/usr/tmp/opencode/spirv-spike/（依赖与主仓库逐字对齐：shaderc build-from-source + spirv-cross2 glsl + spirv-tools use-compiled-tools）
- S0-1 建独立 spike crate
- S0-2 复制 3~5 个代表性 shader 样本（standalone uniform + UBO + sampler + varying + 1 个含未使用 uniform/中间变量的故意样本）
- S0-3 符号共存 + API 冒烟：单进程内 shaderc→optimize→spirv-cross 全链 exit 0 无 abort/segfault；输出 word[0]==0x07230203
- S0-4 OpName 保留：优化前后 spirv-cross 输出 uniform 声明名集合对比（含 UBO 成员名）
- S0-5 文本 diff 预演 + 性能计时：差异仅限死代码移除；optimize 平均耗时 < 20ms/shader 且低于 shaderc 编译耗时

判定：三条件全过才进阶段 1。回退：符号共存失败→方案 B（版本对齐/链接顺序/放弃）；OpName 丢失→砍 AggressiveDCE 只留轻量链

### 阶段 1：管线接入（最小侵入 + fail-open）
- S1-1 chore(deps)：提交 Cargo.toml/lock（spirv-tools 依赖落地）
- S1-2 新建 src/shader_translator/spirv_opt.rs：OPT_PIPELINE_VERSION 常量 + run(spv: &[u32]) -> Result<Vec<u32>, OptError>（防御校验空/magic → opt::create(Some(TargetEnv::Vulkan_1_1)) → register_pass 链 → optimize → as_words().to_vec()；Err 枚举 EmptyInput/BadMagic/Optimize(String)；msg 回调转 log）
- S1-3 改造 spirv_pass.rs:213-215 spirv_pass()：调用 spirv_opt::run，Err → warn + 原样回退（fail-open）
- S1-4 mod.rs 注册模块；cache.rs compute_key 并入 OPT_PIPELINE_VERSION
- S1-5 跑全验证矩阵

Commit：C1 chore(deps) + C2 feat(shader)（可单独 revert C2）

### 阶段 2：Pass 集调优（逐 pass 验证、逐 commit）
- S2-1 确认 AggressiveDCE 的 preserve_interface 语义（crate 无参版本 vs MobileGL false）
- S2-2 EliminateDeadConstant（低风险）
- S2-3 CompactIds（仅重编号不删名）
- S2-4 EliminateDeadMembers（高风险：结构体成员删除触碰 UBO/SSBO std140 布局与 OpMemberName——spike 先行确认对 Block/BufferBlock 不处理才接入）
- S2-5 可选 RedundancyElimination/SimplifyInstructions

每 pass 独立 commit（perf(shader): 启用 X pass），差分 A/B 0 FAIL + 名字保留才保留，否则单 pass revert

### 阶段 3：全验证收尾
- S3-1 全矩阵回归
- S3-2 性能基线：冷缓存 shaderc-only vs +opt 耗时/SPIR-V word 数/GLES 行数
- S3-3 更新 spirv_pass.rs 模块 doc + TODO/ 记录 MobileGL fork 路线
- S3-4 可选真机跑一局

## 2. Pass 链推荐（Rust 可用子集）

核心链（阶段 1 接入）：① AggressiveDCE（删死代码；crate 无参版本默认 preserve_interface=true，差异由 pass 2 补齐）② RemoveUnusedInterfaceVariables（删未使用 in/out/interface）

可选链（阶段 2）：EliminateDeadConstant → CompactIds → EliminateDeadMembers（高风险 spike 先行）→ RedundancyElimination

明确排除：StripDebugInfo（删 OpName 击穿 Zero 保名教训，永不启用）；env 用 Some(TargetEnv::Vulkan_1_1)（对齐 MobileGL）

## 3. 代码改动点

| 文件 | 改动 |
|---|---|
| Cargo.toml:20 | 已有依赖仅提交 |
| src/shader_translator/spirv_opt.rs | 新建（常量+run+单测） |
| src/shader_translator/spirv_pass.rs:213-215 | spirv_pass() 改调 run，Err→warn+回退 |
| src/shader_translator/mod.rs | 注册模块 |
| src/shader_translator/cache.rs:41-93 | key 并入 OPT_PIPELINE_VERSION |

不变式：translate() 永不返回 Failed；fail-open 输出与现状逐字节一致

## 4. Spike 最小用例（4 个：smoke/names/diff/perf）

样本：tests/glsl/、glslang_suite 复制 3~5 个 + 1 个故意含死代码样本

附属确认：AggressiveDCE preserve_interface 实际行为；Vulkan_1_1 vs OpenGL_4_5 env 差异；EliminateDeadMembers 对 Block/BufferBlock 处理

## 5. 验证矩阵

| 项 | 基线 | 目标 |
|---|---|---|
| cargo test | 302 | 不劣化（+spirv_opt 单测） |
| 差分 A（desktop vs translate） | 0 FAIL | 0 FAIL |
| 差分 B（gles vs translate） | 0 FAIL | 0 FAIL |
| glslang_suite | 1080 失败 | 不劣化 |
| nm GL 导出 | 1392 | 不变 |
| 性能 | shaderc-only | +opt 可接受 |

频次：阶段 1/2 每 pass/3 各一轮

## 6. 本期不做的 MobileGL 自研 pass（5 个）

FlattenInterfaceStruct / RenameSamplerFunctionParameter / RenameBuiltinShadowingFunctions / EliminateFloatEqualsZero / DecomposeWorkgroupVec3

原因：① Passes 枚举硬编码 switch 无自定义通道（fork 涉及 LGPL 合规 + C++ 桥维护）② 修的是 MobileGL Vulkan target 产物问题，我们 OpenGL target 大概率不存在（spike 抽查证伪）③ DecomposeWorkgroupVec3 针对 compute，GLES 无 compute 管线

近似替代：标准链已覆盖 MobileGL 公共链 7 pass 中 2 个核心（AggressiveDCE + RemoveUnusedInterfaceVariables）

未来路线：fork spirv-tools-rs C 桥（保留 LGPL 声明）或关注 SPIRV-Tools 官方新 pass

## 7. 依赖与并行性

串行关键路径：C1 → 阶段0 → 阶段1 → 阶段2（pass 逐个）→ 阶段3

并行窗口：① 阶段0 build 预热与样本准备 ② 验证矩阵 5 项并行 ③ S3-2/S3-3 并行

决策门：阶段0 出口三条件全过；阶段2 每 pass 差分 0 FAIL + 名字保留

全局回退：revert 到 spirv_pass() 直通 = 现状，零残留
