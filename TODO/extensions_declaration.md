# 扩展字符串声明决策记录（FAKE_EXTENSIONS）

## 状态：策略已定（2026-08-08，m00271 讨论后维持现状）

## 背景

用户 m00271 问：扩展字符串可以声明吧？实际上支持了一大堆扩展没什么问题吧。
答：**合法且常见**（扩展不依赖 core 版本报告），但**行为依赖类扩展需谨慎**。

## 事故教训（GL_ARB_buffer_storage）

- 曾声明 GL_ARB_buffer_storage → MC 走 BufferStorage 路径 → Adreno 预填数据链路
  断裂 → **UI 塌缩**
- 修复：移除伪造声明（commit `85c6e1e`）——**声明了但实现有坑 = 应用走错误路径**

## 当前策略

`build_fake_extensions`（src/gl/exports.rs:590-616 附近）按 **caps 校验剔除**
行为依赖扩展——不因 stub 导出（阶段 2/3 后 1009 个 stub 已导出）而全量声明。

## 安全可声明子集（查询/微调类，stub 无副作用）

| 扩展 | 说明 |
|---|---|
| GL_ARB_internalformat_query2 | GetInternalformati64v stub 返回 0——app 查询得 0 安全降级 |
| GL_KHR_no_error | 纯声明无行为 |
| GL_ARB_texture_filter_anisotropic | 若启用（过滤参数透传 GLES 原生支持） |
| GL_ARB_separate_shader_objects | stub 无副作用（GLES 无 SSO，app 使用需降级） |

## 行为依赖不建议声明

| 扩展 | 风险 |
|---|---|
| GL_ARB_buffer_storage | Adreno 链路已验证有坑（事故教训） |
| GL_ARB_compute_shader / GL_ARB_shader_image_load_store | stub no-op → 画面缺失 |
| GL_ARB_direct_state_access / GL_ARB_program_interface_query | DSA stub 返回 0 对象 → 应用崩溃风险 |
| GL_ARB_draw_instanced / GL_ARB_base_instance 等 draw 语义扩展 | stub no-op → 不绘制 |
| GL_ARB_transform_feedback* | TF 绑定透传可用但 DrawTransformFeedback stub → 回读绘制缺失 |

## 后续决策点（待办）

- 是否将「安全可声明子集」4 项加入 FAKE_EXTENSIONS（需真机验证 MC 无回归）
- 声明原则总结：**声明 = 承诺行为**——只有实现完整（透传/模拟）的才声明；
  stub 的只声明「查询无害」类
