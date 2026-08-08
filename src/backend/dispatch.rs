use libc::c_char;
use std::ffi::c_void;

#[repr(C)]
#[allow(dead_code)]
pub struct GlesDispatch {
    /// Address of the shared no-op stub used for missing optional functions.
    pub stub: unsafe extern "C" fn(),

    // Direct pass-through functions
    pub clear: unsafe extern "C" fn(u32),
    pub get_string: unsafe extern "C" fn(u32) -> *const c_char,
    pub enable: unsafe extern "C" fn(u32),
    pub disable: unsafe extern "C" fn(u32),
    pub depth_func: unsafe extern "C" fn(u32),
    pub depth_mask: unsafe extern "C" fn(u8),
    pub blend_func: unsafe extern "C" fn(u32, u32),
    pub clear_color: unsafe extern "C" fn(f32, f32, f32, f32),
    pub clear_depth: unsafe extern "C" fn(f32),
    pub clear_stencil: unsafe extern "C" fn(i32),
    pub viewport: unsafe extern "C" fn(i32, i32, i32, i32),
    pub scissor: unsafe extern "C" fn(i32, i32, i32, i32),
    pub cull_face: unsafe extern "C" fn(u32),
    pub front_face: unsafe extern "C" fn(u32),
    pub line_width: unsafe extern "C" fn(f32),
    pub active_texture: unsafe extern "C" fn(u32),
    pub pixel_store_i: unsafe extern "C" fn(u32, i32),
    pub draw_arrays: unsafe extern "C" fn(u32, i32, i32),
    pub draw_elements: unsafe extern "C" fn(u32, i32, u32, *const c_void),
    pub finish: unsafe extern "C" fn(),
    pub flush: unsafe extern "C" fn(),
    pub generate_mipmap: unsafe extern "C" fn(u32),
    pub get_error: unsafe extern "C" fn() -> u32,

    // Buffer
    pub gen_buffers: unsafe extern "C" fn(i32, *mut u32),
    pub delete_buffers: unsafe extern "C" fn(i32, *const u32),
    pub bind_buffer: unsafe extern "C" fn(u32, u32),
    pub buffer_data: unsafe extern "C" fn(u32, isize, *const c_void, u32),
    pub buffer_sub_data: unsafe extern "C" fn(u32, isize, isize, *const c_void),

    // Buffer 高级
    pub map_buffer_range: unsafe extern "C" fn(u32, isize, isize, u32) -> *mut c_void,
    pub unmap_buffer: unsafe extern "C" fn(u32) -> u8,
    pub flush_mapped_buffer_range: unsafe extern "C" fn(u32, isize, isize),
    pub copy_buffer_sub_data: unsafe extern "C" fn(u32, u32, isize, isize, isize),
    pub bind_buffer_base: unsafe extern "C" fn(u32, u32, u32),
    pub bind_buffer_range: unsafe extern "C" fn(u32, u32, u32, isize, isize),
    pub buffer_storage: unsafe extern "C" fn(u32, isize, *const c_void, u32),
    pub get_buffer_sub_data: unsafe extern "C" fn(u32, isize, isize, *mut c_void),
    pub get_buffer_parameter_iv: unsafe extern "C" fn(u32, u32, *mut i32),
    pub get_buffer_pointer_v: unsafe extern "C" fn(u32, u32, *mut *mut c_void),
    pub is_buffer: unsafe extern "C" fn(u32) -> u8,

    // Buffer → Texture 绑定（GL_EXT_texture_buffer / GLES 3.2）
    pub tex_buffer: unsafe extern "C" fn(u32, u32, u32),
    pub tex_buffer_range: unsafe extern "C" fn(u32, u32, u32, isize, isize),

    // Debug 输出控制
    pub debug_message_control: unsafe extern "C" fn(u32, u32, u32, i32, *const u32, u8),

    // VAO
    pub gen_vertex_arrays: unsafe extern "C" fn(i32, *mut u32),
    pub delete_vertex_arrays: unsafe extern "C" fn(i32, *const u32),
    pub bind_vertex_array: unsafe extern "C" fn(u32),
    pub enable_vertex_attrib_array: unsafe extern "C" fn(u32),
    pub disable_vertex_attrib_array: unsafe extern "C" fn(u32),
    pub vertex_attrib_pointer: unsafe extern "C" fn(u32, i32, u32, u8, i32, *const c_void),
    pub vertex_attrib_i_pointer: unsafe extern "C" fn(u32, i32, u32, i32, *const c_void),
    pub vertex_attrib_1f: unsafe extern "C" fn(u32, f32),
    pub vertex_attrib_2f: unsafe extern "C" fn(u32, f32, f32),
    pub vertex_attrib_3f: unsafe extern "C" fn(u32, f32, f32, f32),
    pub vertex_attrib_4f: unsafe extern "C" fn(u32, f32, f32, f32, f32),
    pub vertex_attrib_1fv: unsafe extern "C" fn(u32, *const f32),
    pub vertex_attrib_2fv: unsafe extern "C" fn(u32, *const f32),
    pub vertex_attrib_3fv: unsafe extern "C" fn(u32, *const f32),
    pub vertex_attrib_4fv: unsafe extern "C" fn(u32, *const f32),
    pub vertex_attrib_i_1i: unsafe extern "C" fn(u32, i32),
    pub vertex_attrib_i_2i: unsafe extern "C" fn(u32, i32, i32),
    pub vertex_attrib_i_3i: unsafe extern "C" fn(u32, i32, i32, i32),
    pub vertex_attrib_i_4i: unsafe extern "C" fn(u32, i32, i32, i32, i32),
    pub vertex_attrib_i_1ui: unsafe extern "C" fn(u32, u32),
    pub vertex_attrib_i_2ui: unsafe extern "C" fn(u32, u32, u32),
    pub vertex_attrib_i_3ui: unsafe extern "C" fn(u32, u32, u32, u32),
    pub vertex_attrib_i_4ui: unsafe extern "C" fn(u32, u32, u32, u32, u32),
    pub vertex_attrib_i_1iv: unsafe extern "C" fn(u32, *const i32),
    pub vertex_attrib_i_2iv: unsafe extern "C" fn(u32, *const i32),
    pub vertex_attrib_i_3iv: unsafe extern "C" fn(u32, *const i32),
    pub vertex_attrib_i_4iv: unsafe extern "C" fn(u32, *const i32),
    pub vertex_attrib_i_1uiv: unsafe extern "C" fn(u32, *const u32),
    pub vertex_attrib_i_2uiv: unsafe extern "C" fn(u32, *const u32),
    pub vertex_attrib_i_3uiv: unsafe extern "C" fn(u32, *const u32),
    pub vertex_attrib_i_4uiv: unsafe extern "C" fn(u32, *const u32),

    // VAO/Draw 高级（ARB_vertex_attrib_binding / GLES 3.1）
    pub bind_vertex_buffer: unsafe extern "C" fn(u32, u32, isize, i32),
    pub vertex_attrib_format: unsafe extern "C" fn(u32, i32, u32, u8, u32),
    pub vertex_attrib_i_format: unsafe extern "C" fn(u32, i32, u32, u32),
    pub vertex_attrib_binding: unsafe extern "C" fn(u32, u32),
    pub get_vertex_attrib_fv: unsafe extern "C" fn(u32, u32, *mut f32),

    // VAO/Draw 高级
    pub vertex_attrib_divisor: unsafe extern "C" fn(u32, u32),
    pub draw_arrays_instanced: unsafe extern "C" fn(u32, i32, i32, i32),
    pub draw_elements_instanced: unsafe extern "C" fn(u32, i32, u32, *const c_void, i32),
    pub draw_range_elements: unsafe extern "C" fn(u32, u32, u32, i32, u32, *const c_void),
    pub primitive_restart_index: unsafe extern "C" fn(u32),

    // Shader
    pub create_shader: unsafe extern "C" fn(u32) -> u32,
    pub delete_shader: unsafe extern "C" fn(u32),
    pub shader_source: unsafe extern "C" fn(u32, i32, *const *const c_char, *const i32),
    pub compile_shader: unsafe extern "C" fn(u32),
    pub get_shader_iv: unsafe extern "C" fn(u32, u32, *mut i32),
    pub get_shader_info_log: unsafe extern "C" fn(u32, i32, *mut i32, *mut c_char),
    pub gl_create_shader_programv: unsafe extern "C" fn(u32, i32, *const *const c_char) -> u32,

    // Program
    pub create_program: unsafe extern "C" fn() -> u32,
    pub delete_program: unsafe extern "C" fn(u32),
    pub attach_shader: unsafe extern "C" fn(u32, u32),
    pub link_program: unsafe extern "C" fn(u32),
    pub use_program: unsafe extern "C" fn(u32),
    pub get_program_iv: unsafe extern "C" fn(u32, u32, *mut i32),
    pub get_program_info_log: unsafe extern "C" fn(u32, i32, *mut i32, *mut c_char),
    pub get_uniform_location: unsafe extern "C" fn(u32, *const c_char) -> i32,
    pub get_attrib_location: unsafe extern "C" fn(u32, *const c_char) -> i32,
    pub get_frag_data_location: unsafe extern "C" fn(u32, *const c_char) -> i32,
    pub uniform_1f: unsafe extern "C" fn(i32, f32),
    pub uniform_1i: unsafe extern "C" fn(i32, i32),
    pub uniform_2f: unsafe extern "C" fn(i32, f32, f32),
    pub uniform_3f: unsafe extern "C" fn(i32, f32, f32, f32),
    pub uniform_4f: unsafe extern "C" fn(i32, f32, f32, f32, f32),
    pub uniform_2i: unsafe extern "C" fn(i32, i32, i32),
    pub uniform_3i: unsafe extern "C" fn(i32, i32, i32, i32),
    pub uniform_4i: unsafe extern "C" fn(i32, i32, i32, i32, i32),
    pub uniform_1fv: unsafe extern "C" fn(i32, i32, *const f32),
    pub uniform_2fv: unsafe extern "C" fn(i32, i32, *const f32),
    pub uniform_3fv: unsafe extern "C" fn(i32, i32, *const f32),
    pub uniform_4fv: unsafe extern "C" fn(i32, i32, *const f32),
    pub uniform_1iv: unsafe extern "C" fn(i32, i32, *const i32),
    pub uniform_2iv: unsafe extern "C" fn(i32, i32, *const i32),
    pub uniform_3iv: unsafe extern "C" fn(i32, i32, *const i32),
    pub uniform_4iv: unsafe extern "C" fn(i32, i32, *const i32),
    pub uniform_matrix_2fv: unsafe extern "C" fn(i32, i32, u8, *const f32),
    pub uniform_matrix_3fv: unsafe extern "C" fn(i32, i32, u8, *const f32),
    pub uniform_matrix_4fv: unsafe extern "C" fn(i32, i32, u8, *const f32),

    // Shader/Program 高级
    pub detach_shader: unsafe extern "C" fn(u32, u32),
    pub validate_program: unsafe extern "C" fn(u32),
    pub get_active_uniform:
        unsafe extern "C" fn(u32, u32, i32, *mut i32, *mut i32, *mut u32, *mut c_char),
    pub get_active_attrib:
        unsafe extern "C" fn(u32, u32, i32, *mut i32, *mut i32, *mut u32, *mut c_char),
    pub get_uniform_fv: unsafe extern "C" fn(u32, i32, *mut f32),
    pub get_uniform_iv: unsafe extern "C" fn(u32, i32, *mut i32),
    pub get_attached_shaders: unsafe extern "C" fn(u32, i32, *mut i32, *mut u32),
    pub get_shader_source: unsafe extern "C" fn(u32, i32, *mut i32, *mut c_char),
    pub bind_attrib_location: unsafe extern "C" fn(u32, u32, *const c_char),
    pub transform_feedback_varyings: unsafe extern "C" fn(u32, i32, *const *const c_char, u32),
    pub get_transform_feedback_varying:
        unsafe extern "C" fn(u32, u32, i32, *mut i32, *mut i32, *mut u32, *mut c_char),
    pub uniform_block_binding: unsafe extern "C" fn(u32, u32, u32),
    pub get_uniform_block_index: unsafe extern "C" fn(u32, *const c_char) -> u32,
    pub get_active_uniform_block_iv: unsafe extern "C" fn(u32, u32, u32, *mut i32),
    pub get_active_uniform_block_name: unsafe extern "C" fn(u32, u32, i32, *mut i32, *mut c_char),
    pub get_uniform_indices: unsafe extern "C" fn(u32, i32, *const *const c_char, *mut u32),
    pub get_active_uniforms_iv: unsafe extern "C" fn(u32, i32, *const u32, u32, *mut i32),
    pub is_shader: unsafe extern "C" fn(u32) -> u8,
    pub is_program: unsafe extern "C" fn(u32) -> u8,
    pub release_shader_compiler: unsafe extern "C" fn(),

    // Texture
    pub gen_textures: unsafe extern "C" fn(i32, *mut u32),
    pub delete_textures: unsafe extern "C" fn(i32, *const u32),
    pub bind_texture: unsafe extern "C" fn(u32, u32),
    pub tex_image_2d: unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void),
    pub tex_sub_image_2d:
        unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, u32, *const c_void),
    pub tex_parameter_i: unsafe extern "C" fn(u32, u32, i32),

    // Texture 高级
    pub tex_image_3d:
        unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, i32, u32, u32, *const c_void),
    pub tex_sub_image_3d:
        unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, i32, i32, u32, u32, *const c_void),
    pub tex_storage_2d: unsafe extern "C" fn(u32, i32, u32, i32, i32),
    pub tex_storage_3d: unsafe extern "C" fn(u32, i32, u32, i32, i32, i32),
    pub tex_parameter_f: unsafe extern "C" fn(u32, u32, f32),
    pub tex_parameter_fv: unsafe extern "C" fn(u32, u32, *const f32),
    pub tex_parameter_iv: unsafe extern "C" fn(u32, u32, *const i32),
    pub compressed_tex_image_2d:
        unsafe extern "C" fn(u32, i32, u32, i32, i32, i32, i32, *const c_void),
    pub compressed_tex_sub_image_2d:
        unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, u32, i32, *const c_void),
    pub compressed_tex_image_3d:
        unsafe extern "C" fn(u32, i32, u32, i32, i32, i32, i32, i32, *const c_void),
    pub compressed_tex_sub_image_3d:
        unsafe extern "C" fn(u32, i32, i32, i32, i32, i32, i32, i32, u32, i32, *const c_void),
    pub get_tex_image: unsafe extern "C" fn(u32, i32, u32, u32, *mut c_void),
    pub get_tex_level_parameter_iv: unsafe extern "C" fn(u32, i32, u32, *mut i32),
    pub get_tex_parameter_iv: unsafe extern "C" fn(u32, u32, *mut i32),
    pub is_texture: unsafe extern "C" fn(u32) -> u8,

    // Framebuffer / Renderbuffer
    pub gen_framebuffers: unsafe extern "C" fn(i32, *mut u32),
    pub delete_framebuffers: unsafe extern "C" fn(i32, *const u32),
    pub bind_framebuffer: unsafe extern "C" fn(u32, u32),
    pub framebuffer_texture_2d: unsafe extern "C" fn(u32, u32, u32, u32, i32),
    pub framebuffer_texture_layer: unsafe extern "C" fn(u32, u32, u32, i32, i32),
    pub framebuffer_renderbuffer: unsafe extern "C" fn(u32, u32, u32, u32),
    pub check_framebuffer_status: unsafe extern "C" fn(u32) -> u32,
    pub gen_renderbuffers: unsafe extern "C" fn(i32, *mut u32),
    pub delete_renderbuffers: unsafe extern "C" fn(i32, *const u32),
    pub bind_renderbuffer: unsafe extern "C" fn(u32, u32),
    pub renderbuffer_storage: unsafe extern "C" fn(u32, u32, i32, i32),
    pub renderbuffer_storage_multisample: unsafe extern "C" fn(u32, i32, u32, i32, i32),
    pub blit_framebuffer: unsafe extern "C" fn(i32, i32, i32, i32, i32, i32, i32, i32, u32, u32),
    pub draw_buffers: unsafe extern "C" fn(i32, *const u32),
    pub read_buffer: unsafe extern "C" fn(u32),
    pub read_pixels: unsafe extern "C" fn(i32, i32, i32, i32, u32, u32, *mut c_void),
    pub clear_buffer_fv: unsafe extern "C" fn(u32, i32, *const f32),
    pub clear_buffer_iv: unsafe extern "C" fn(u32, i32, *const i32),
    pub clear_buffer_uiv: unsafe extern "C" fn(u32, i32, *const u32),
    pub clear_buffer_fi: unsafe extern "C" fn(u32, i32, f32, i32),
    pub get_framebuffer_attachment_parameter_iv: unsafe extern "C" fn(u32, u32, u32, *mut i32),
    pub is_framebuffer: unsafe extern "C" fn(u32) -> u8,
    pub is_renderbuffer: unsafe extern "C" fn(u32) -> u8,

    // State
    pub enable_i: unsafe extern "C" fn(u32, u32),
    pub disable_i: unsafe extern "C" fn(u32, u32),
    pub blend_func_separate: unsafe extern "C" fn(u32, u32, u32, u32),
    pub blend_equation: unsafe extern "C" fn(u32),
    pub blend_equation_separate: unsafe extern "C" fn(u32, u32),
    pub blend_func_i: unsafe extern "C" fn(u32, u32, u32),
    pub blend_func_separate_i: unsafe extern "C" fn(u32, u32, u32, u32, u32),
    pub blend_equation_i: unsafe extern "C" fn(u32, u32),
    pub blend_equation_separate_i: unsafe extern "C" fn(u32, u32, u32),
    pub color_mask: unsafe extern "C" fn(u8, u8, u8, u8),
    pub color_mask_i: unsafe extern "C" fn(u32, u8, u8, u8, u8),
    pub depth_range_f: unsafe extern "C" fn(f32, f32),
    pub stencil_func: unsafe extern "C" fn(u32, i32, u32),
    pub stencil_func_separate: unsafe extern "C" fn(u32, u32, i32, u32),
    pub stencil_op: unsafe extern "C" fn(u32, u32, u32),
    pub stencil_op_separate: unsafe extern "C" fn(u32, u32, u32, u32),
    pub stencil_mask: unsafe extern "C" fn(u32),
    pub stencil_mask_separate: unsafe extern "C" fn(u32, u32),
    pub polygon_offset: unsafe extern "C" fn(f32, f32),
    pub polygon_mode: unsafe extern "C" fn(u32, u32),
    pub pixel_store_f: unsafe extern "C" fn(u32, f32),
    pub point_parameter_f: unsafe extern "C" fn(u32, f32),
    pub scissor_indexed: unsafe extern "C" fn(u32, i32, i32, i32, i32),
    pub viewport_indexed: unsafe extern "C" fn(u32, f32, f32, f32, f32),
    pub is_enabled: unsafe extern "C" fn(u32) -> u8,
    pub is_enabled_i: unsafe extern "C" fn(u32, u32) -> u8,

    // Drawing
    pub multi_draw_arrays: unsafe extern "C" fn(u32, *const i32, *const i32, i32),
    pub multi_draw_elements: unsafe extern "C" fn(u32, *const i32, u32, *const *const c_void, i32),
    // GLES 3.2 / EXT_draw_buffers_indexed base vertex & base instance 系列
    pub draw_elements_base_vertex: unsafe extern "C" fn(u32, i32, u32, *const c_void, i32),
    pub draw_range_elements_base_vertex:
        unsafe extern "C" fn(u32, u32, u32, i32, u32, *const c_void, i32),
    pub draw_elements_instanced_base_vertex:
        unsafe extern "C" fn(u32, i32, u32, *const c_void, i32, i32),
    pub draw_elements_instanced_base_instance:
        unsafe extern "C" fn(u32, i32, u32, *const c_void, i32, u32),
    pub draw_elements_instanced_base_vertex_base_instance:
        unsafe extern "C" fn(u32, i32, u32, *const c_void, i32, i32, u32),
    pub draw_arrays_instanced_base_instance: unsafe extern "C" fn(u32, i32, i32, i32, u32),
    pub multi_draw_elements_base_vertex:
        unsafe extern "C" fn(u32, *const i32, u32, *const *const c_void, i32, *const i32),
    // GLES 3.1 indirect draw
    pub draw_arrays_indirect: unsafe extern "C" fn(u32, *const c_void),
    pub draw_elements_indirect: unsafe extern "C" fn(u32, u32, *const c_void),
    // GLES 3.2 multi-draw indirect
    // stride 为 GLsizei（i32）：桌面与 GLES 签名一致（C7 修正 isize→i32）
    pub multi_draw_arrays_indirect: unsafe extern "C" fn(u32, *const c_void, i32, i32),
    pub multi_draw_elements_indirect: unsafe extern "C" fn(u32, u32, *const c_void, i32, i32),
    // GL 4.6 / GL_ARB_indirect_compute indirect count（GLES 几乎无支持，stub 时告警）
    // drawcount 参数为 GLintptr（buffer offset，isize）；maxdrawcount/stride 为 GLsizei（i32）
    pub multi_draw_arrays_indirect_count:
        unsafe extern "C" fn(u32, *const c_void, isize, i32, i32),
    pub multi_draw_elements_indirect_count:
        unsafe extern "C" fn(u32, u32, *const c_void, isize, i32, i32),

    // Query
    pub gen_queries: unsafe extern "C" fn(i32, *mut u32),
    pub delete_queries: unsafe extern "C" fn(i32, *const u32),
    pub is_query: unsafe extern "C" fn(u32) -> u8,
    pub begin_query: unsafe extern "C" fn(u32, u32),
    pub end_query: unsafe extern "C" fn(u32),
    pub get_query_iv: unsafe extern "C" fn(u32, u32, *mut i32),
    pub get_query_object_iv: unsafe extern "C" fn(u32, u32, *mut i32),
    pub get_query_object_uiv: unsafe extern "C" fn(u32, u32, *mut u32),

    // Sync
    pub fence_sync: unsafe extern "C" fn(u32, u32) -> *mut c_void,
    pub delete_sync: unsafe extern "C" fn(*mut c_void),
    pub client_wait_sync: unsafe extern "C" fn(*mut c_void, u32, u64) -> u32,
    pub wait_sync: unsafe extern "C" fn(*mut c_void, u32, u64),
    pub is_sync: unsafe extern "C" fn(*mut c_void) -> u8,

    // Transform Feedback
    pub gen_transform_feedbacks: unsafe extern "C" fn(i32, *mut u32),
    pub delete_transform_feedbacks: unsafe extern "C" fn(i32, *const u32),
    pub bind_transform_feedback: unsafe extern "C" fn(u32, u32),
    pub begin_transform_feedback: unsafe extern "C" fn(u32),
    pub end_transform_feedback: unsafe extern "C" fn(),
    pub pause_transform_feedback: unsafe extern "C" fn(),
    pub resume_transform_feedback: unsafe extern "C" fn(),
    pub is_transform_feedback: unsafe extern "C" fn(u32) -> u8,

    // Compute（GLES 3.1 core；P2 atomic→SSBO 模拟的运行时载体）
    pub dispatch_compute: unsafe extern "C" fn(u32, u32, u32),
    pub memory_barrier: unsafe extern "C" fn(u32),

    // Sampler
    pub gen_samplers: unsafe extern "C" fn(i32, *mut u32),
    pub delete_samplers: unsafe extern "C" fn(i32, *const u32),
    pub bind_sampler: unsafe extern "C" fn(u32, u32),
    pub sampler_parameter_i: unsafe extern "C" fn(u32, u32, i32),
    pub sampler_parameter_f: unsafe extern "C" fn(u32, u32, f32),
    pub sampler_parameter_iv: unsafe extern "C" fn(u32, u32, *const i32),
    pub sampler_parameter_fv: unsafe extern "C" fn(u32, u32, *const f32),
    pub is_sampler: unsafe extern "C" fn(u32) -> u8,

    // 其他
    pub get_integerv: unsafe extern "C" fn(u32, *mut i32),
    pub get_string_i: unsafe extern "C" fn(u32, u32) -> *const c_char,

    // 其他状态查询
    pub get_boolean_v: unsafe extern "C" fn(u32, *mut u8),
    pub get_float_v: unsafe extern "C" fn(u32, *mut f32),
    pub get_double_v: unsafe extern "C" fn(u32, *mut f64),
    pub get_integer_64v: unsafe extern "C" fn(u32, *mut i64),
    pub get_booleani_v: unsafe extern "C" fn(u32, u32, *mut u8),
    pub get_integeri_v: unsafe extern "C" fn(u32, u32, *mut i32),
    pub get_floati_v: unsafe extern "C" fn(u32, u32, *mut f32),
    pub get_doublei_v: unsafe extern "C" fn(u32, u32, *mut f64),
}

// 编译期约束：GlesDispatch 必须全部为函数指针（全指针布局），且字段总数精确匹配。
// 字段数 = 266（stub 槽 + 265 个 GL 函数指针；P2 新增 dispatch_compute/memory_barrier）。
// ⚠️ 若结构体字段增减，必须同步更新此处 264，否则编译失败——防止显式初始化
// 遗漏字段时静默出错（原 % 断言只能捕获"总大小非 8 倍数"，捕获不了字段数漂移）。
const _: () = assert!(
    std::mem::size_of::<GlesDispatch>() == 266 * std::mem::size_of::<unsafe extern "C" fn()>()
);

// —— 按签名类别的安全 no-op stub（零参数，忽略入参，返回安全常量）——
// 用于 all_stub()：GLES 库加载失败时，宿主调用这些槽位拿到的是安全值
// （null / 0 / 空串），而非 AArch64 x0 残留垃圾（与 egl_sys 侧 P1-A 同类问题）。
// 命名带 gl_ 前缀，与 egl_sys 模块的 stub_* 区分（避免两个模块概念混淆）。
#[allow(dead_code)]
unsafe extern "C" fn gl_stub_void() {}
unsafe extern "C" fn gl_stub_zero_u32() -> u32 {
    0
}
unsafe extern "C" fn gl_stub_zero_i32() -> i32 {
    0 // get_uniform_location / get_attrib_location：0 = 无此 attribute/uniform
}
unsafe extern "C" fn gl_stub_zero_u8() -> u8 {
    0 // GL_FALSE：宿主 is_* 查询返回 false，走低配路径（安全）
}
unsafe extern "C" fn gl_stub_null_ptr() -> *mut c_void {
    std::ptr::null_mut()
}
#[allow(clippy::manual_c_str_literals)]
unsafe extern "C" fn gl_stub_empty_string() -> *const c_char {
    b"\0".as_ptr() as *const c_char
}

/// 按签名类别将零参数 stub（先 reify 为自身签名的 fn pointer）transmute 为
/// 目标字段签名。stub 不使用入参（忽略寄存器/栈上的参数），转换安全。
/// 空臂：void 返回字段（目标签名尾部无返回类型）。
/// ⚠️ 必须先 reify：fn item（gl_stub_void）是零大小类型，直接 transmute 触发
/// E0591（can't transmute zero-sized type）；reify 为函数指针（8 字节）后
/// fn pointer → fn pointer 的 transmute 才合法。
macro_rules! stub {
    () => {{
        let f: unsafe extern "C" fn() = gl_stub_void;
        unsafe {
            // transmute 是 stub 初始化的固有产物（M1 按签名 stub 填充）：先 reify 为
            // 自身签名 fn pointer 再转换到目标字段签名。missing_transmute_annotations：
            // 未标注显式目标类型（由字段类型推断）；useless_transmute：字段签名恰为
            // 自身签名（stub 槽）时 clippy 视为自身到自身转换。两者均安全，显式豁免。
            #[allow(clippy::missing_transmute_annotations, clippy::useless_transmute)]
            std::mem::transmute::<_, _>(f)
        }
    }};
    ($e:expr) => {{
        unsafe {
            #[allow(clippy::missing_transmute_annotations, clippy::useless_transmute)]
            std::mem::transmute::<_, _>($e)
        }
    }};
}

impl GlesDispatch {
    /// Create a dispatch table where every function pointer is a safe no-op stub.
    /// Used as a fallback when the real GLES library fails to load, so that
    /// exported C functions don't panic/abort the host process.
    ///
    /// 按签名类别显式初始化（M1 修复，替代原 MaybeUninit 逐槽填充）：
    /// - void 返回 → gl_stub_void（no-op）
    /// - 返回 u32（GLenum/GLuint/状态查询/创建类）→ gl_stub_zero_u32（0）
    /// - 返回 i32（location 查询）→ gl_stub_zero_i32（0）
    /// - 返回 u8（GLboolean is_* 查询）→ gl_stub_zero_u8（GL_FALSE=0，安全低配路径）
    /// - 返回 *mut c_void（创建/映射类）→ gl_stub_null_ptr（null，宿主可判空）
    /// - 返回 *const c_char（字符串查询）→ gl_stub_empty_string（空串，宿主可安全 CStr 解析）
    pub fn all_stub() -> Self {
        Self {
            // 占位 stub 槽
            stub: stub!(),
            clear: stub!(),
            get_string: stub!(gl_stub_empty_string as unsafe extern "C" fn() -> *const c_char),
            enable: stub!(),
            disable: stub!(),
            depth_func: stub!(),
            depth_mask: stub!(),
            blend_func: stub!(),
            clear_color: stub!(),
            clear_depth: stub!(),
            clear_stencil: stub!(),
            viewport: stub!(),
            scissor: stub!(),
            cull_face: stub!(),
            front_face: stub!(),
            line_width: stub!(),
            active_texture: stub!(),
            pixel_store_i: stub!(),
            draw_arrays: stub!(),
            draw_elements: stub!(),
            finish: stub!(),
            flush: stub!(),
            generate_mipmap: stub!(),
            get_error: stub!(gl_stub_zero_u32 as unsafe extern "C" fn() -> u32),

            // Buffer
            gen_buffers: stub!(),
            delete_buffers: stub!(),
            bind_buffer: stub!(),
            buffer_data: stub!(),
            buffer_sub_data: stub!(),

            // Buffer 高级
            map_buffer_range: stub!(gl_stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
            unmap_buffer: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),
            flush_mapped_buffer_range: stub!(),
            copy_buffer_sub_data: stub!(),
            bind_buffer_base: stub!(),
            bind_buffer_range: stub!(),
            buffer_storage: stub!(),
            get_buffer_sub_data: stub!(),
            get_buffer_parameter_iv: stub!(),
            get_buffer_pointer_v: stub!(),
            is_buffer: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),

            // Buffer → Texture 绑定（GL_EXT_texture_buffer / GLES 3.2）
            tex_buffer: stub!(),
            tex_buffer_range: stub!(),

            // Debug 输出控制
            debug_message_control: stub!(),

            // VAO
            gen_vertex_arrays: stub!(),
            delete_vertex_arrays: stub!(),
            bind_vertex_array: stub!(),
            enable_vertex_attrib_array: stub!(),
            disable_vertex_attrib_array: stub!(),
            vertex_attrib_pointer: stub!(),
            vertex_attrib_i_pointer: stub!(),
            vertex_attrib_1f: stub!(),
            vertex_attrib_2f: stub!(),
            vertex_attrib_3f: stub!(),
            vertex_attrib_4f: stub!(),
            vertex_attrib_1fv: stub!(),
            vertex_attrib_2fv: stub!(),
            vertex_attrib_3fv: stub!(),
            vertex_attrib_4fv: stub!(),
            vertex_attrib_i_1i: stub!(),
            vertex_attrib_i_2i: stub!(),
            vertex_attrib_i_3i: stub!(),
            vertex_attrib_i_4i: stub!(),
            vertex_attrib_i_1ui: stub!(),
            vertex_attrib_i_2ui: stub!(),
            vertex_attrib_i_3ui: stub!(),
            vertex_attrib_i_4ui: stub!(),
            vertex_attrib_i_1iv: stub!(),
            vertex_attrib_i_2iv: stub!(),
            vertex_attrib_i_3iv: stub!(),
            vertex_attrib_i_4iv: stub!(),
            vertex_attrib_i_1uiv: stub!(),
            vertex_attrib_i_2uiv: stub!(),
            vertex_attrib_i_3uiv: stub!(),
            vertex_attrib_i_4uiv: stub!(),

            // VAO/Draw 高级（ARB_vertex_attrib_binding / GLES 3.1）
            bind_vertex_buffer: stub!(),
            vertex_attrib_format: stub!(),
            vertex_attrib_i_format: stub!(),
            vertex_attrib_binding: stub!(),
            get_vertex_attrib_fv: stub!(),

            // VAO/Draw 高级
            vertex_attrib_divisor: stub!(),
            draw_arrays_instanced: stub!(),
            draw_elements_instanced: stub!(),
            draw_range_elements: stub!(),
            primitive_restart_index: stub!(),

            // Shader
            create_shader: stub!(gl_stub_zero_u32 as unsafe extern "C" fn() -> u32),
            delete_shader: stub!(),
            shader_source: stub!(),
            compile_shader: stub!(),
            get_shader_iv: stub!(),
            get_shader_info_log: stub!(),
            gl_create_shader_programv: stub!(gl_stub_zero_u32 as unsafe extern "C" fn() -> u32),

            // Program
            create_program: stub!(gl_stub_zero_u32 as unsafe extern "C" fn() -> u32),
            delete_program: stub!(),
            attach_shader: stub!(),
            link_program: stub!(),
            use_program: stub!(),
            get_program_iv: stub!(),
            get_program_info_log: stub!(),
            get_uniform_location: stub!(gl_stub_zero_i32 as unsafe extern "C" fn() -> i32),
            get_attrib_location: stub!(gl_stub_zero_i32 as unsafe extern "C" fn() -> i32),
            get_frag_data_location: stub!(gl_stub_zero_i32 as unsafe extern "C" fn() -> i32),
            uniform_1f: stub!(),
            uniform_1i: stub!(),
            uniform_2f: stub!(),
            uniform_3f: stub!(),
            uniform_4f: stub!(),
            uniform_2i: stub!(),
            uniform_3i: stub!(),
            uniform_4i: stub!(),
            uniform_1fv: stub!(),
            uniform_2fv: stub!(),
            uniform_3fv: stub!(),
            uniform_4fv: stub!(),
            uniform_1iv: stub!(),
            uniform_2iv: stub!(),
            uniform_3iv: stub!(),
            uniform_4iv: stub!(),
            uniform_matrix_2fv: stub!(),
            uniform_matrix_3fv: stub!(),
            uniform_matrix_4fv: stub!(),

            // Shader/Program 高级
            detach_shader: stub!(),
            validate_program: stub!(),
            get_active_uniform: stub!(),
            get_active_attrib: stub!(),
            get_uniform_fv: stub!(),
            get_uniform_iv: stub!(),
            get_attached_shaders: stub!(),
            get_shader_source: stub!(),
            bind_attrib_location: stub!(),
            transform_feedback_varyings: stub!(),
            get_transform_feedback_varying: stub!(),
            uniform_block_binding: stub!(),
            get_uniform_block_index: stub!(gl_stub_zero_u32 as unsafe extern "C" fn() -> u32),
            get_active_uniform_block_iv: stub!(),
            get_active_uniform_block_name: stub!(),
            get_uniform_indices: stub!(),
            get_active_uniforms_iv: stub!(),
            is_shader: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),
            is_program: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),
            release_shader_compiler: stub!(),

            // Texture
            gen_textures: stub!(),
            delete_textures: stub!(),
            bind_texture: stub!(),
            tex_image_2d: stub!(),
            tex_sub_image_2d: stub!(),
            tex_parameter_i: stub!(),

            // Texture 高级
            tex_image_3d: stub!(),
            tex_sub_image_3d: stub!(),
            tex_storage_2d: stub!(),
            tex_storage_3d: stub!(),
            tex_parameter_f: stub!(),
            tex_parameter_fv: stub!(),
            tex_parameter_iv: stub!(),
            compressed_tex_image_2d: stub!(),
            compressed_tex_sub_image_2d: stub!(),
            compressed_tex_image_3d: stub!(),
            compressed_tex_sub_image_3d: stub!(),
            get_tex_image: stub!(),
            get_tex_level_parameter_iv: stub!(),
            get_tex_parameter_iv: stub!(),
            is_texture: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),

            // Framebuffer / Renderbuffer
            gen_framebuffers: stub!(),
            delete_framebuffers: stub!(),
            bind_framebuffer: stub!(),
            framebuffer_texture_2d: stub!(),
            framebuffer_texture_layer: stub!(),
            framebuffer_renderbuffer: stub!(),
            check_framebuffer_status: stub!(gl_stub_zero_u32 as unsafe extern "C" fn() -> u32),
            gen_renderbuffers: stub!(),
            delete_renderbuffers: stub!(),
            bind_renderbuffer: stub!(),
            renderbuffer_storage: stub!(),
            renderbuffer_storage_multisample: stub!(),
            blit_framebuffer: stub!(),
            draw_buffers: stub!(),
            read_buffer: stub!(),
            read_pixels: stub!(),
            clear_buffer_fv: stub!(),
            clear_buffer_iv: stub!(),
            clear_buffer_uiv: stub!(),
            clear_buffer_fi: stub!(),
            get_framebuffer_attachment_parameter_iv: stub!(),
            is_framebuffer: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),
            is_renderbuffer: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),

            // State
            enable_i: stub!(),
            disable_i: stub!(),
            blend_func_separate: stub!(),
            blend_equation: stub!(),
            blend_equation_separate: stub!(),
            blend_func_i: stub!(),
            blend_func_separate_i: stub!(),
            blend_equation_i: stub!(),
            blend_equation_separate_i: stub!(),
            color_mask: stub!(),
            color_mask_i: stub!(),
            depth_range_f: stub!(),
            stencil_func: stub!(),
            stencil_func_separate: stub!(),
            stencil_op: stub!(),
            stencil_op_separate: stub!(),
            stencil_mask: stub!(),
            stencil_mask_separate: stub!(),
            polygon_offset: stub!(),
            polygon_mode: stub!(),
            pixel_store_f: stub!(),
            point_parameter_f: stub!(),
            scissor_indexed: stub!(),
            viewport_indexed: stub!(),
            is_enabled: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),
            is_enabled_i: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),

            // Drawing
            multi_draw_arrays: stub!(),
            multi_draw_elements: stub!(),
            draw_elements_base_vertex: stub!(),
            draw_range_elements_base_vertex: stub!(),
            draw_elements_instanced_base_vertex: stub!(),
            draw_elements_instanced_base_instance: stub!(),
            draw_elements_instanced_base_vertex_base_instance: stub!(),
            draw_arrays_instanced_base_instance: stub!(),
            multi_draw_elements_base_vertex: stub!(),
            draw_arrays_indirect: stub!(),
            draw_elements_indirect: stub!(),
            multi_draw_arrays_indirect: stub!(),
            multi_draw_elements_indirect: stub!(),
            multi_draw_arrays_indirect_count: stub!(),
            multi_draw_elements_indirect_count: stub!(),

            // Query
            gen_queries: stub!(),
            delete_queries: stub!(),
            is_query: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),
            begin_query: stub!(),
            end_query: stub!(),
            get_query_iv: stub!(),
            get_query_object_iv: stub!(),
            get_query_object_uiv: stub!(),

            // Sync
            fence_sync: stub!(gl_stub_null_ptr as unsafe extern "C" fn() -> *mut c_void),
            delete_sync: stub!(),
            client_wait_sync: stub!(gl_stub_zero_u32 as unsafe extern "C" fn() -> u32),
            wait_sync: stub!(),
            is_sync: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),

            // Transform Feedback
            gen_transform_feedbacks: stub!(),
            delete_transform_feedbacks: stub!(),
            bind_transform_feedback: stub!(),
            begin_transform_feedback: stub!(),
            end_transform_feedback: stub!(),
            pause_transform_feedback: stub!(),
            resume_transform_feedback: stub!(),
            is_transform_feedback: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),
            dispatch_compute: stub!(),
            memory_barrier: stub!(),

            // Sampler
            gen_samplers: stub!(),
            delete_samplers: stub!(),
            bind_sampler: stub!(),
            sampler_parameter_i: stub!(),
            sampler_parameter_f: stub!(),
            sampler_parameter_iv: stub!(),
            sampler_parameter_fv: stub!(),
            is_sampler: stub!(gl_stub_zero_u8 as unsafe extern "C" fn() -> u8),

            // 其他
            get_integerv: stub!(),
            get_string_i: stub!(gl_stub_empty_string as unsafe extern "C" fn() -> *const c_char),

            // 其他状态查询
            get_boolean_v: stub!(),
            get_float_v: stub!(),
            get_double_v: stub!(),
            get_integer_64v: stub!(),
            get_booleani_v: stub!(),
            get_integeri_v: stub!(),
            get_floati_v: stub!(),
            get_doublei_v: stub!(),
        }
    }

    #[allow(clippy::missing_transmute_annotations)]
    pub fn load_from(loader: &super::loader::GlesLoader) -> Option<Self> {
        unsafe extern "C" fn unimplemented_stub() {}

        macro_rules! load {
            ($name:expr) => {{
                let ptr = loader.get_proc($name);
                if ptr.is_null() {
                    log::warn!("[GlesDispatch] failed to load required function: {}", $name);
                    return None;
                }
                ptr
            }};
        }

        macro_rules! load_opt {
            ($name:expr) => {{
                let ptr = loader.get_proc($name);
                if ptr.is_null() {
                    // GLES 驱动未导出的可选函数：降为 debug，避免启动期刷屏。
                    // 这些函数在拦截层会通过 is_stub 检测后走模拟或占位逻辑。
                    log::debug!(
                        "[GlesDispatch] optional function not available: {} (will emulate/stub)",
                        $name
                    );
                    unsafe { std::mem::transmute::<unsafe extern "C" fn(), _>(unimplemented_stub) }
                } else {
                    unsafe { std::mem::transmute(ptr) }
                }
            }};
        }

        // 按 core / OES / EXT 后缀顺序尝试加载扩展函数。
        // GLES 驱动常以带后缀的符号名（如 glDrawElementsBaseVertexOES）导出扩展函数，
        // 仅试 core 名会误判为不支持。capabilities 层会基于扩展字符串做权威能力检测。
        // C2：OES/EXT 后缀名走 get_proc_gles（dlsym 失败兜底 eglGetProcAddress——
        // 部分驱动如 Mesa 不导出这些符号为 dlsym）；core 名保持纯 dlsym：
        // core 名可能是桌面独有（如 glMultiDrawArrays），eglGetProcAddress 会
        // 返回桌面 GL 入口，在 GLES context 上调用产生 INVALID_OPERATION。
        macro_rules! load_opt_suffixes {
            ($core:expr, $oes:expr, $ext:expr) => {{
                let mut ptr = loader.get_proc($core);
                if ptr.is_null() && !$oes.is_empty() {
                    ptr = loader.get_proc_gles($oes);
                }
                if ptr.is_null() && !$ext.is_empty() {
                    ptr = loader.get_proc_gles($ext);
                }
                if ptr.is_null() {
                    log::debug!(
                        "[GlesDispatch] extension function not available: {} / {} / {} (will emulate/stub)",
                        $core, $oes, $ext
                    );
                    unsafe { std::mem::transmute::<unsafe extern "C" fn(), _>(unimplemented_stub) }
                } else {
                    unsafe { std::mem::transmute(ptr) }
                }
            }};
        }

        Some(Self {
            stub: unimplemented_stub,
            clear: unsafe { std::mem::transmute(load!("glClear")) },
            get_string: unsafe { std::mem::transmute(load!("glGetString")) },
            enable: unsafe { std::mem::transmute(load!("glEnable")) },
            disable: unsafe { std::mem::transmute(load!("glDisable")) },
            depth_func: unsafe { std::mem::transmute(load!("glDepthFunc")) },
            depth_mask: unsafe { std::mem::transmute(load!("glDepthMask")) },
            blend_func: unsafe { std::mem::transmute(load!("glBlendFunc")) },
            clear_color: unsafe { std::mem::transmute(load!("glClearColor")) },
            clear_depth: unsafe { std::mem::transmute(load!("glClearDepthf")) },
            clear_stencil: unsafe { std::mem::transmute(load!("glClearStencil")) },
            viewport: unsafe { std::mem::transmute(load!("glViewport")) },
            scissor: unsafe { std::mem::transmute(load!("glScissor")) },
            cull_face: unsafe { std::mem::transmute(load!("glCullFace")) },
            front_face: unsafe { std::mem::transmute(load!("glFrontFace")) },
            line_width: unsafe { std::mem::transmute(load!("glLineWidth")) },
            active_texture: unsafe { std::mem::transmute(load!("glActiveTexture")) },
            pixel_store_i: unsafe { std::mem::transmute(load!("glPixelStorei")) },
            draw_arrays: unsafe { std::mem::transmute(load!("glDrawArrays")) },
            draw_elements: unsafe { std::mem::transmute(load!("glDrawElements")) },
            finish: unsafe { std::mem::transmute(load!("glFinish")) },
            flush: unsafe { std::mem::transmute(load!("glFlush")) },
            generate_mipmap: unsafe { std::mem::transmute(load!("glGenerateMipmap")) },
            get_error: unsafe { std::mem::transmute(load!("glGetError")) },

            gen_buffers: unsafe { std::mem::transmute(load!("glGenBuffers")) },
            delete_buffers: unsafe { std::mem::transmute(load!("glDeleteBuffers")) },
            bind_buffer: unsafe { std::mem::transmute(load!("glBindBuffer")) },
            buffer_data: unsafe { std::mem::transmute(load!("glBufferData")) },
            buffer_sub_data: unsafe { std::mem::transmute(load!("glBufferSubData")) },

            map_buffer_range: unsafe { std::mem::transmute(load!("glMapBufferRange")) },
            unmap_buffer: unsafe { std::mem::transmute(load!("glUnmapBuffer")) },
            flush_mapped_buffer_range: unsafe {
                std::mem::transmute(load!("glFlushMappedBufferRange"))
            },
            copy_buffer_sub_data: unsafe { std::mem::transmute(load!("glCopyBufferSubData")) },
            bind_buffer_base: unsafe { std::mem::transmute(load!("glBindBufferBase")) },
            bind_buffer_range: unsafe { std::mem::transmute(load!("glBindBufferRange")) },
            buffer_storage: unsafe {
                let ptr = loader.get_proc("glBufferStorage");
                let ptr = if ptr.is_null() {
                    loader.get_proc("glBufferStorageEXT")
                } else {
                    ptr
                };
                if ptr.is_null() {
                    std::mem::transmute::<unsafe extern "C" fn(), _>(unimplemented_stub)
                } else {
                    std::mem::transmute(ptr)
                }
            },
            get_buffer_sub_data: load_opt!("glGetBufferSubData"),
            get_buffer_parameter_iv: unsafe {
                std::mem::transmute(load!("glGetBufferParameteriv"))
            },
            get_buffer_pointer_v: load_opt!("glGetBufferPointerv"),
            is_buffer: unsafe { std::mem::transmute(load!("glIsBuffer")) },

            // GL_EXT_texture_buffer / GLES 3.2
            tex_buffer: load_opt!("glTexBuffer"),
            tex_buffer_range: load_opt!("glTexBufferRange"),
            // GL_KHR_debug
            debug_message_control: load_opt!("glDebugMessageControl"),

            gen_vertex_arrays: unsafe { std::mem::transmute(load!("glGenVertexArrays")) },
            delete_vertex_arrays: unsafe { std::mem::transmute(load!("glDeleteVertexArrays")) },
            bind_vertex_array: unsafe { std::mem::transmute(load!("glBindVertexArray")) },
            enable_vertex_attrib_array: unsafe {
                std::mem::transmute(load!("glEnableVertexAttribArray"))
            },
            disable_vertex_attrib_array: unsafe {
                std::mem::transmute(load!("glDisableVertexAttribArray"))
            },
            vertex_attrib_pointer: unsafe { std::mem::transmute(load!("glVertexAttribPointer")) },
            vertex_attrib_i_pointer: unsafe {
                std::mem::transmute(load!("glVertexAttribIPointer"))
            },
            vertex_attrib_1f: unsafe { std::mem::transmute(load!("glVertexAttrib1f")) },
            vertex_attrib_2f: unsafe { std::mem::transmute(load!("glVertexAttrib2f")) },
            vertex_attrib_3f: unsafe { std::mem::transmute(load!("glVertexAttrib3f")) },
            vertex_attrib_4f: unsafe { std::mem::transmute(load!("glVertexAttrib4f")) },
            vertex_attrib_1fv: unsafe { std::mem::transmute(load!("glVertexAttrib1fv")) },
            vertex_attrib_2fv: unsafe { std::mem::transmute(load!("glVertexAttrib2fv")) },
            vertex_attrib_3fv: unsafe { std::mem::transmute(load!("glVertexAttrib3fv")) },
            vertex_attrib_4fv: unsafe { std::mem::transmute(load!("glVertexAttrib4fv")) },
            vertex_attrib_i_1i: load_opt!("glVertexAttribI1i"),
            vertex_attrib_i_2i: load_opt!("glVertexAttribI2i"),
            vertex_attrib_i_3i: load_opt!("glVertexAttribI3i"),
            vertex_attrib_i_4i: unsafe { std::mem::transmute(load!("glVertexAttribI4i")) },
            vertex_attrib_i_1ui: load_opt!("glVertexAttribI1ui"),
            vertex_attrib_i_2ui: load_opt!("glVertexAttribI2ui"),
            vertex_attrib_i_3ui: load_opt!("glVertexAttribI3ui"),
            vertex_attrib_i_4ui: unsafe { std::mem::transmute(load!("glVertexAttribI4ui")) },
            vertex_attrib_i_1iv: load_opt!("glVertexAttribI1iv"),
            vertex_attrib_i_2iv: load_opt!("glVertexAttribI2iv"),
            vertex_attrib_i_3iv: load_opt!("glVertexAttribI3iv"),
            vertex_attrib_i_4iv: unsafe { std::mem::transmute(load!("glVertexAttribI4iv")) },
            vertex_attrib_i_1uiv: load_opt!("glVertexAttribI1uiv"),
            vertex_attrib_i_2uiv: load_opt!("glVertexAttribI2uiv"),
            vertex_attrib_i_3uiv: load_opt!("glVertexAttribI3uiv"),
            vertex_attrib_i_4uiv: unsafe { std::mem::transmute(load!("glVertexAttribI4uiv")) },

            vertex_attrib_divisor: unsafe { std::mem::transmute(load!("glVertexAttribDivisor")) },
            draw_arrays_instanced: unsafe { std::mem::transmute(load!("glDrawArraysInstanced")) },
            draw_elements_instanced: unsafe {
                std::mem::transmute(load!("glDrawElementsInstanced"))
            },
            draw_range_elements: load_opt!("glDrawRangeElements"),
            primitive_restart_index: load_opt!("glPrimitiveRestartIndex"),

            // ARB_vertex_attrib_binding / GLES 3.1
            bind_vertex_buffer: load_opt!("glBindVertexBuffer"),
            vertex_attrib_format: load_opt!("glVertexAttribFormat"),
            vertex_attrib_i_format: load_opt!("glVertexAttribIFormat"),
            vertex_attrib_binding: load_opt!("glVertexAttribBinding"),
            get_vertex_attrib_fv: load_opt!("glGetVertexAttribfv"),

            create_shader: unsafe { std::mem::transmute(load!("glCreateShader")) },
            delete_shader: unsafe { std::mem::transmute(load!("glDeleteShader")) },
            shader_source: unsafe { std::mem::transmute(load!("glShaderSource")) },
            compile_shader: unsafe { std::mem::transmute(load!("glCompileShader")) },
            get_shader_iv: unsafe { std::mem::transmute(load!("glGetShaderiv")) },
            get_shader_info_log: unsafe { std::mem::transmute(load!("glGetShaderInfoLog")) },
            gl_create_shader_programv: load_opt!("glCreateShaderProgramv"),

            create_program: unsafe { std::mem::transmute(load!("glCreateProgram")) },
            delete_program: unsafe { std::mem::transmute(load!("glDeleteProgram")) },
            attach_shader: unsafe { std::mem::transmute(load!("glAttachShader")) },
            link_program: unsafe { std::mem::transmute(load!("glLinkProgram")) },
            use_program: unsafe { std::mem::transmute(load!("glUseProgram")) },
            get_program_iv: unsafe { std::mem::transmute(load!("glGetProgramiv")) },
            get_program_info_log: unsafe { std::mem::transmute(load!("glGetProgramInfoLog")) },
            get_uniform_location: unsafe { std::mem::transmute(load!("glGetUniformLocation")) },
            get_attrib_location: unsafe { std::mem::transmute(load!("glGetAttribLocation")) },
            // GLES 3.0 core 原生符号（非 load_opt：GLES 3.0 规范保证存在）
            get_frag_data_location: unsafe { std::mem::transmute(load!("glGetFragDataLocation")) },
            uniform_1f: unsafe { std::mem::transmute(load!("glUniform1f")) },
            uniform_1i: unsafe { std::mem::transmute(load!("glUniform1i")) },
            uniform_2f: unsafe { std::mem::transmute(load!("glUniform2f")) },
            uniform_3f: unsafe { std::mem::transmute(load!("glUniform3f")) },
            uniform_4f: unsafe { std::mem::transmute(load!("glUniform4f")) },
            uniform_2i: unsafe { std::mem::transmute(load!("glUniform2i")) },
            uniform_3i: unsafe { std::mem::transmute(load!("glUniform3i")) },
            uniform_4i: unsafe { std::mem::transmute(load!("glUniform4i")) },
            uniform_1fv: unsafe { std::mem::transmute(load!("glUniform1fv")) },
            uniform_2fv: unsafe { std::mem::transmute(load!("glUniform2fv")) },
            uniform_3fv: unsafe { std::mem::transmute(load!("glUniform3fv")) },
            uniform_4fv: unsafe { std::mem::transmute(load!("glUniform4fv")) },
            uniform_1iv: unsafe { std::mem::transmute(load!("glUniform1iv")) },
            uniform_2iv: unsafe { std::mem::transmute(load!("glUniform2iv")) },
            uniform_3iv: unsafe { std::mem::transmute(load!("glUniform3iv")) },
            uniform_4iv: unsafe { std::mem::transmute(load!("glUniform4iv")) },
            uniform_matrix_2fv: unsafe { std::mem::transmute(load!("glUniformMatrix2fv")) },
            uniform_matrix_3fv: unsafe { std::mem::transmute(load!("glUniformMatrix3fv")) },
            uniform_matrix_4fv: unsafe { std::mem::transmute(load!("glUniformMatrix4fv")) },

            detach_shader: unsafe { std::mem::transmute(load!("glDetachShader")) },
            validate_program: unsafe { std::mem::transmute(load!("glValidateProgram")) },
            get_active_uniform: unsafe { std::mem::transmute(load!("glGetActiveUniform")) },
            get_active_attrib: unsafe { std::mem::transmute(load!("glGetActiveAttrib")) },
            get_uniform_fv: unsafe { std::mem::transmute(load!("glGetUniformfv")) },
            get_uniform_iv: unsafe { std::mem::transmute(load!("glGetUniformiv")) },
            get_attached_shaders: unsafe { std::mem::transmute(load!("glGetAttachedShaders")) },
            get_shader_source: unsafe { std::mem::transmute(load!("glGetShaderSource")) },
            bind_attrib_location: unsafe { std::mem::transmute(load!("glBindAttribLocation")) },
            transform_feedback_varyings: unsafe {
                std::mem::transmute(load!("glTransformFeedbackVaryings"))
            },
            get_transform_feedback_varying: unsafe {
                std::mem::transmute(load!("glGetTransformFeedbackVarying"))
            },
            uniform_block_binding: unsafe { std::mem::transmute(load!("glUniformBlockBinding")) },
            get_uniform_block_index: unsafe {
                std::mem::transmute(load!("glGetUniformBlockIndex"))
            },
            get_active_uniform_block_iv: unsafe {
                std::mem::transmute(load!("glGetActiveUniformBlockiv"))
            },
            get_active_uniform_block_name: unsafe {
                std::mem::transmute(load!("glGetActiveUniformBlockName"))
            },
            get_uniform_indices: unsafe { std::mem::transmute(load!("glGetUniformIndices")) },
            get_active_uniforms_iv: unsafe { std::mem::transmute(load!("glGetActiveUniformsiv")) },
            is_shader: unsafe { std::mem::transmute(load!("glIsShader")) },
            is_program: unsafe { std::mem::transmute(load!("glIsProgram")) },
            release_shader_compiler: unsafe {
                std::mem::transmute(load!("glReleaseShaderCompiler"))
            },

            gen_textures: unsafe { std::mem::transmute(load!("glGenTextures")) },
            delete_textures: unsafe { std::mem::transmute(load!("glDeleteTextures")) },
            bind_texture: unsafe { std::mem::transmute(load!("glBindTexture")) },
            tex_image_2d: unsafe { std::mem::transmute(load!("glTexImage2D")) },
            tex_sub_image_2d: unsafe { std::mem::transmute(load!("glTexSubImage2D")) },
            tex_parameter_i: unsafe { std::mem::transmute(load!("glTexParameteri")) },

            tex_image_3d: unsafe { std::mem::transmute(load!("glTexImage3D")) },
            tex_sub_image_3d: unsafe { std::mem::transmute(load!("glTexSubImage3D")) },
            tex_storage_2d: unsafe { std::mem::transmute(load!("glTexStorage2D")) },
            tex_storage_3d: unsafe { std::mem::transmute(load!("glTexStorage3D")) },
            tex_parameter_f: unsafe { std::mem::transmute(load!("glTexParameterf")) },
            tex_parameter_fv: unsafe { std::mem::transmute(load!("glTexParameterfv")) },
            tex_parameter_iv: unsafe { std::mem::transmute(load!("glTexParameteriv")) },
            compressed_tex_image_2d: unsafe {
                std::mem::transmute(load!("glCompressedTexImage2D"))
            },
            compressed_tex_sub_image_2d: unsafe {
                std::mem::transmute(load!("glCompressedTexSubImage2D"))
            },
            compressed_tex_image_3d: unsafe {
                std::mem::transmute(load!("glCompressedTexImage3D"))
            },
            compressed_tex_sub_image_3d: unsafe {
                std::mem::transmute(load!("glCompressedTexSubImage3D"))
            },
            get_tex_image: load_opt!("glGetTexImage"),
            get_tex_level_parameter_iv: unsafe {
                std::mem::transmute(load!("glGetTexLevelParameteriv"))
            },
            get_tex_parameter_iv: unsafe { std::mem::transmute(load!("glGetTexParameteriv")) },
            is_texture: unsafe { std::mem::transmute(load!("glIsTexture")) },

            gen_framebuffers: unsafe { std::mem::transmute(load!("glGenFramebuffers")) },
            delete_framebuffers: unsafe { std::mem::transmute(load!("glDeleteFramebuffers")) },
            bind_framebuffer: unsafe { std::mem::transmute(load!("glBindFramebuffer")) },
            framebuffer_texture_2d: unsafe { std::mem::transmute(load!("glFramebufferTexture2D")) },
            framebuffer_texture_layer: unsafe {
                std::mem::transmute(load!("glFramebufferTextureLayer"))
            },
            framebuffer_renderbuffer: unsafe {
                std::mem::transmute(load!("glFramebufferRenderbuffer"))
            },
            check_framebuffer_status: unsafe {
                std::mem::transmute(load!("glCheckFramebufferStatus"))
            },
            gen_renderbuffers: unsafe { std::mem::transmute(load!("glGenRenderbuffers")) },
            delete_renderbuffers: unsafe { std::mem::transmute(load!("glDeleteRenderbuffers")) },
            bind_renderbuffer: unsafe { std::mem::transmute(load!("glBindRenderbuffer")) },
            renderbuffer_storage: unsafe { std::mem::transmute(load!("glRenderbufferStorage")) },
            renderbuffer_storage_multisample: unsafe {
                std::mem::transmute(load!("glRenderbufferStorageMultisample"))
            },
            blit_framebuffer: unsafe { std::mem::transmute(load!("glBlitFramebuffer")) },
            draw_buffers: unsafe { std::mem::transmute(load!("glDrawBuffers")) },
            read_buffer: unsafe { std::mem::transmute(load!("glReadBuffer")) },
            read_pixels: unsafe { std::mem::transmute(load!("glReadPixels")) },
            clear_buffer_fv: unsafe { std::mem::transmute(load!("glClearBufferfv")) },
            clear_buffer_iv: unsafe { std::mem::transmute(load!("glClearBufferiv")) },
            clear_buffer_uiv: unsafe { std::mem::transmute(load!("glClearBufferuiv")) },
            clear_buffer_fi: unsafe { std::mem::transmute(load!("glClearBufferfi")) },
            get_framebuffer_attachment_parameter_iv: unsafe {
                std::mem::transmute(load!("glGetFramebufferAttachmentParameteriv"))
            },
            is_framebuffer: unsafe { std::mem::transmute(load!("glIsFramebuffer")) },
            is_renderbuffer: unsafe { std::mem::transmute(load!("glIsRenderbuffer")) },

            enable_i: unsafe { std::mem::transmute(load!("glEnablei")) },
            disable_i: unsafe { std::mem::transmute(load!("glDisablei")) },
            blend_func_separate: unsafe { std::mem::transmute(load!("glBlendFuncSeparate")) },
            blend_equation: unsafe { std::mem::transmute(load!("glBlendEquation")) },
            blend_equation_separate: unsafe {
                std::mem::transmute(load!("glBlendEquationSeparate"))
            },
            blend_func_i: unsafe { std::mem::transmute(load!("glBlendFunci")) },
            blend_func_separate_i: unsafe { std::mem::transmute(load!("glBlendFuncSeparatei")) },
            blend_equation_i: unsafe { std::mem::transmute(load!("glBlendEquationi")) },
            blend_equation_separate_i: unsafe {
                std::mem::transmute(load!("glBlendEquationSeparatei"))
            },
            color_mask: unsafe { std::mem::transmute(load!("glColorMask")) },
            color_mask_i: unsafe { std::mem::transmute(load!("glColorMaski")) },
            depth_range_f: unsafe { std::mem::transmute(load!("glDepthRangef")) },
            stencil_func: unsafe { std::mem::transmute(load!("glStencilFunc")) },
            stencil_func_separate: unsafe { std::mem::transmute(load!("glStencilFuncSeparate")) },
            stencil_op: unsafe { std::mem::transmute(load!("glStencilOp")) },
            stencil_op_separate: unsafe { std::mem::transmute(load!("glStencilOpSeparate")) },
            stencil_mask: unsafe { std::mem::transmute(load!("glStencilMask")) },
            stencil_mask_separate: unsafe { std::mem::transmute(load!("glStencilMaskSeparate")) },
            polygon_offset: unsafe { std::mem::transmute(load!("glPolygonOffset")) },
            polygon_mode: load_opt!("glPolygonMode"),
            pixel_store_f: load_opt!("glPixelStoref"),
            point_parameter_f: load_opt!("glPointParameterf"),
            scissor_indexed: load_opt!("glScissorIndexed"),
            viewport_indexed: load_opt!("glViewportIndexedf"),
            is_enabled: unsafe { std::mem::transmute(load!("glIsEnabled")) },
            is_enabled_i: unsafe { std::mem::transmute(load!("glIsEnabledi")) },

            // GLES 无 core glMultiDrawArrays/Elements，仅 GL_EXT_multi_draw_arrays
            // 提供 EXT 后缀名（Adreno/Mesa 均支持）——core 名几乎必然 stub，
            // 补 EXT 后缀恢复原生透传（C4）
            multi_draw_arrays: load_opt_suffixes!(
                "glMultiDrawArrays",
                "",
                "glMultiDrawArraysEXT"
            ),
            multi_draw_elements: load_opt_suffixes!(
                "glMultiDrawElements",
                "",
                "glMultiDrawElementsEXT"
            ),
            // GLES 3.2 / GL_OES_draw_elements_base_vertex：base vertex 系列
            draw_elements_base_vertex: load_opt_suffixes!(
                "glDrawElementsBaseVertex",
                "glDrawElementsBaseVertexOES",
                "glDrawElementsBaseVertexEXT"
            ),
            draw_range_elements_base_vertex: load_opt_suffixes!(
                "glDrawRangeElementsBaseVertex",
                "glDrawRangeElementsBaseVertexOES",
                ""
            ),
            draw_elements_instanced_base_vertex: load_opt_suffixes!(
                "glDrawElementsInstancedBaseVertex",
                "glDrawElementsInstancedBaseVertexOES",
                ""
            ),
            // GLES 3.2 / GL_EXT_base_instance：base instance 系列
            draw_elements_instanced_base_instance: load_opt_suffixes!(
                "glDrawElementsInstancedBaseInstance",
                "",
                "glDrawElementsInstancedBaseInstanceEXT"
            ),
            draw_elements_instanced_base_vertex_base_instance: load_opt_suffixes!(
                "glDrawElementsInstancedBaseVertexBaseInstance",
                "",
                "glDrawElementsInstancedBaseVertexBaseInstanceEXT"
            ),
            draw_arrays_instanced_base_instance: load_opt_suffixes!(
                "glDrawArraysInstancedBaseInstance",
                "",
                "glDrawArraysInstancedBaseInstanceEXT"
            ),
            // GLES 3.2 / GL_EXT_multi_draw_elements_base_vertex
            multi_draw_elements_base_vertex: load_opt_suffixes!(
                "glMultiDrawElementsBaseVertex",
                "",
                "glMultiDrawElementsBaseVertexEXT"
            ),
            // GLES 3.1 indirect draw（core，无扩展后缀）
            draw_arrays_indirect: load_opt!("glDrawArraysIndirect"),
            draw_elements_indirect: load_opt!("glDrawElementsIndirect"),
            // GLES 3.2 / GL_EXT_multi_draw_indirect
            multi_draw_arrays_indirect: load_opt_suffixes!(
                "glMultiDrawArraysIndirect",
                "",
                "glMultiDrawArraysIndirectEXT"
            ),
            multi_draw_elements_indirect: load_opt_suffixes!(
                "glMultiDrawElementsIndirect",
                "",
                "glMultiDrawElementsIndirectEXT"
            ),
            // GL 4.6 / GL_ARB_indirect_compute...：GLES 无对应扩展，几乎必然 stub
            multi_draw_arrays_indirect_count: load_opt!("glMultiDrawArraysIndirectCount"),
            multi_draw_elements_indirect_count: load_opt!("glMultiDrawElementsIndirectCount"),

            gen_queries: unsafe { std::mem::transmute(load!("glGenQueries")) },
            delete_queries: unsafe { std::mem::transmute(load!("glDeleteQueries")) },
            is_query: unsafe { std::mem::transmute(load!("glIsQuery")) },
            begin_query: unsafe { std::mem::transmute(load!("glBeginQuery")) },
            end_query: unsafe { std::mem::transmute(load!("glEndQuery")) },
            get_query_iv: unsafe { std::mem::transmute(load!("glGetQueryiv")) },
            get_query_object_iv: load_opt!("glGetQueryObjectiv"),
            get_query_object_uiv: unsafe { std::mem::transmute(load!("glGetQueryObjectuiv")) },

            fence_sync: unsafe { std::mem::transmute(load!("glFenceSync")) },
            delete_sync: unsafe { std::mem::transmute(load!("glDeleteSync")) },
            client_wait_sync: unsafe { std::mem::transmute(load!("glClientWaitSync")) },
            wait_sync: unsafe { std::mem::transmute(load!("glWaitSync")) },
            is_sync: unsafe { std::mem::transmute(load!("glIsSync")) },

            gen_transform_feedbacks: unsafe {
                std::mem::transmute(load!("glGenTransformFeedbacks"))
            },
            delete_transform_feedbacks: unsafe {
                std::mem::transmute(load!("glDeleteTransformFeedbacks"))
            },
            bind_transform_feedback: unsafe {
                std::mem::transmute(load!("glBindTransformFeedback"))
            },
            begin_transform_feedback: unsafe {
                std::mem::transmute(load!("glBeginTransformFeedback"))
            },
            end_transform_feedback: unsafe { std::mem::transmute(load!("glEndTransformFeedback")) },
            pause_transform_feedback: unsafe {
                std::mem::transmute(load!("glPauseTransformFeedback"))
            },
            resume_transform_feedback: unsafe {
                std::mem::transmute(load!("glResumeTransformFeedback"))
            },
            is_transform_feedback: unsafe { std::mem::transmute(load!("glIsTransformFeedback")) },
            // GLES 3.1 core（项目前提，必选加载）
            dispatch_compute: unsafe { std::mem::transmute(load!("glDispatchCompute")) },
            memory_barrier: unsafe { std::mem::transmute(load!("glMemoryBarrier")) },

            gen_samplers: unsafe { std::mem::transmute(load!("glGenSamplers")) },
            delete_samplers: unsafe { std::mem::transmute(load!("glDeleteSamplers")) },
            bind_sampler: unsafe { std::mem::transmute(load!("glBindSampler")) },
            sampler_parameter_i: unsafe { std::mem::transmute(load!("glSamplerParameteri")) },
            sampler_parameter_f: unsafe { std::mem::transmute(load!("glSamplerParameterf")) },
            sampler_parameter_iv: unsafe { std::mem::transmute(load!("glSamplerParameteriv")) },
            sampler_parameter_fv: unsafe { std::mem::transmute(load!("glSamplerParameterfv")) },
            is_sampler: unsafe { std::mem::transmute(load!("glIsSampler")) },

            get_integerv: unsafe { std::mem::transmute(load!("glGetIntegerv")) },
            get_string_i: unsafe { std::mem::transmute(load!("glGetStringi")) },

            get_boolean_v: unsafe { std::mem::transmute(load!("glGetBooleanv")) },
            get_float_v: unsafe { std::mem::transmute(load!("glGetFloatv")) },
            get_double_v: load_opt!("glGetDoublev"),
            get_integer_64v: load_opt!("glGetInteger64v"),
            get_booleani_v: load_opt!("glGetBooleani_v"),
            get_integeri_v: load_opt!("glGetIntegeri_v"),
            get_floati_v: load_opt!("glGetFloati_v"),
            get_doublei_v: load_opt!("glGetDoublei_v"),
        })
    }
}
