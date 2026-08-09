//! Shader 翻译缓存模块
//!
//! 使用 SHA256(source + stage + gles_version) 作为 key，
//! LRU 策略淘汰，避免重复执行 shaderc + spirv-cross 翻译流程。

use lru::LruCache;
use sha2::{Digest, Sha256};
use std::num::NonZeroUsize;
use std::sync::Mutex;

/// 缓存 key：SHA256 哈希值的字节数组
type CacheKey = [u8; 32];

/// Shader 翻译缓存
///
/// 缓存翻译后的 GLSL ES 源码，命中时直接返回避免重复翻译。
/// 默认容量 64 个条目（足够缓存 MC 的所有 shader）。
pub struct ShaderCache {
    cache: Mutex<LruCache<CacheKey, String>>,
}

impl ShaderCache {
    /// 创建指定容量的缓存
    pub fn new(capacity: usize) -> Self {
        Self {
            cache: Mutex::new(LruCache::new(
                NonZeroUsize::new(capacity).unwrap_or_else(|| NonZeroUsize::new(64).unwrap()),
            )),
        }
    }

    /// 计算缓存 key
    ///
    /// 动态生成缓存键，考虑更多因素以提高精确性：
    /// 1. 源码内容
    /// 2. Shader 阶段
    /// 3. GLES 版本
    /// 4. 源码特征（如是否包含 samplerBuffer、textureQueryLod 等）
    /// 5. 预处理后的特征（如注入的 location/binding 数量）
    /// 6. 优化后的特征（如是否启用了特定优化）
    fn compute_key(source: &str, stage: u32, gles_version: u32) -> CacheKey {
        let mut hasher = Sha256::new();

        // 基础信息
        hasher.update(source.as_bytes());
        hasher.update(stage.to_le_bytes());
        hasher.update(gles_version.to_le_bytes());
        // pass 链版本：变更 spirv_opt pass 链时必须递增 OPT_PIPELINE_VERSION，
        // 否则旧缓存（不同优化产物）会错误命中（S1-4）
        hasher.update(super::spirv_opt::OPT_PIPELINE_VERSION.to_le_bytes());

        // 源码特征
        let features = [
            source.contains("samplerBuffer"),
            source.contains("textureQueryLod"),
            source.contains("atomic_uint"),
            source.contains("image"),
            source.contains("UBO"),
            source.contains("SSBO"),
            source.contains("gl_VertexID"),
            source.contains("gl_FragColor"),
        ];

        for feature in features {
            hasher.update([if feature { 1u8 } else { 0u8 }]);
        }

        // 预处理特征（基于源码内容估计）
        let estimated_injects: u32 = if source.contains("samplerBuffer") {
            1
        } else {
            0
        } + if source.contains("textureQueryLod") {
            1
        } else {
            0
        } + if source.contains("atomic_uint") { 1 } else { 0 };

        hasher.update(estimated_injects.to_le_bytes());

        // 优化特征
        let optimizations = [
            source.contains("#version 450 core"), // 版本升级特征
            source.contains("layout(location="),  // location 注入特征
            source.contains("layout(binding="),   // binding 注入特征
        ];

        for opt in optimizations {
            hasher.update([if opt { 1u8 } else { 0u8 }]);
        }

        let result = hasher.finalize();
        let mut key = [0u8; 32];
        key.copy_from_slice(&result);
        key
    }

    /// 查询缓存
    /// 返回 Some(essl) 如果命中，None 如果未命中
    pub fn get(&self, source: &str, stage: u32, gles_version: u32) -> Option<String> {
        let key = Self::compute_key(source, stage, gles_version);
        let mut cache = self.cache.lock().unwrap();
        cache.get(&key).cloned()
    }

    /// 插入缓存
    pub fn put(&self, source: &str, stage: u32, gles_version: u32, essl: String) {
        let key = Self::compute_key(source, stage, gles_version);
        let mut cache = self.cache.lock().unwrap();
        cache.put(key, essl);
    }
}

/// 全局缓存实例（延迟初始化）
static SHADER_CACHE: std::sync::OnceLock<ShaderCache> = std::sync::OnceLock::new();

/// 获取全局缓存实例
pub fn global_cache() -> &'static ShaderCache {
    SHADER_CACHE.get_or_init(|| ShaderCache::new(64))
}
