# ANGLE 后端问题（真机）待办

## 状态：搁置（用户 m00228「先不猜了，暂时放着先」；记录于 2026-08-08）

## 现象（log/latest_game.log，491 行，08-07 16:58，FLUORATEGL_BACKEND=angle，启动约 3 秒崩溃）

- **ArithmeticException / by zero**：MC hnx UBO 池构造读 `glGetIntegerv(GL_UNIFORM_BUFFER_OFFSET_ALIGNMENT=0x8A34)` 返回 **0**（正常应为 16/256）→ 对齐除零崩溃
- `glGetString(GL_VERSION/RENDERER/VENDOR)` 三个全返回 **(null)**（capabilities 检测 version=0）
- `GL_MAX_TEXTURE_SIZE` 等查询返回 0
- **26 个 GLES 3.0/3.1 标准函数**（glGetBufferSubData、glVertexAttribI* 等）日志报 `optional function not available`——GLES 函数表大面积缺失

## 静态审计结论（五层链路候选，按可能性排序）

1. **EGL context 与 GLES 函数表不匹配**（最大嫌疑）：EGL 层走 ANGLE 创建 context，但 GLES 函数从 dlopen 的库 dlsym——若 context 与函数表非同源，glGetString/glGetIntegerv 返回 null/0
2. ~~dlopen 固定库名不匹配~~（**已排除**）：config.rs:38-44/64-67/84-87 用 `libEGL_angle.so`/`libGLESv2_angle.so` 固定名；用户澄清 FCL 的 ANGLE 模式把 EGL 直连 ANGLE——且 MobileGlues 库名与我们一致（不是库名问题）
3. 版本不配对（EGL 1.4 报告 vs ANGLE 实际）

## 盲区（当前无日志可定位）

- `backend/loader.rs` + `egl_sys/loader.rs`：dlopen 失败无 dlerror、成功无真实路径记录（dladdr）
- `egl/exports.rs`：导出函数无调用日志（无法确认 FCL 是否走我们的 EGL 导出）

## 待办（恢复时执行）

1. 精读 491 行日志，定位 GLES 函数表缺失的根源
2. 最小日志增强：
   - `egl/exports.rs` 四函数（eglGetDisplay / eglInitialize / eglChooseConfig / eglCreateContext / eglMakeCurrent）加参数 + 返回值日志
   - `backend/loader.rs` + `egl_sys/loader.rs`：dlopen 失败打印 dlerror()；成功后 dladdr(handle) 打印真实解析路径（确认 dlopen 到的是不是 FCL 的 ANGLE）
3. 参考 MobileGlues 的 `init_target_egl`：自建临时 context 自检模式（加载后立即验证函数表完整性）

## 关联

- capabilities 检测失败（version=0）曾触发 C1 修复（supported 判定改为以 is_stub 为主导 + version 兜底）——GLES 3.2 特性已不受 version=0 影响，但本问题导致的是更基础的函数表缺失
