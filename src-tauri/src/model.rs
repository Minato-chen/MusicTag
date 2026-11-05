// MusicTag — 领域模型（纯 serde 契约，无业务逻辑）。
//
// 跨 IPC 的 Rust/TS 契约形状在此冻结（design.md §10.1）：字段逐字保留
// `rename_all`/snake_case/`#[serde(rename="sidecar")]` 注解，任何改动都会
// 破坏前端 `src/api/types.ts` 与集成测试的序列化断言。

use serde::{Deserialize, Serialize};

/// 只读列表项。字段均 `String`，Rust 侧不 trim。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongSummary {
    pub path: String,
    pub title: String,
    pub artist: String,
}

/// 目录监听的失效通知；前端按目录和 watchId 过滤后重新读取列表。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderChanged {
    pub dir: String,
    pub watch_id: u64,
    pub error: Option<String>,
}

/// 缺失字段筛选维度（missing-fields-filter）。
///
/// 序列化字面量是前端 IPC 契约的一部分；业务检查顺序由 service 层固定为
/// `title → artist → album → cover → lyrics`，不取决于调用方传入顺序。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MissingField {
    Title,
    Artist,
    Album,
    #[serde(rename = "cover")]
    Cover,
    Lyrics,
}

/// 单首歌曲的缺失字段结果。只返回路径和维度，不携带完整 Song、封面或歌词正文。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingSong {
    pub path: String,
    pub missing: Vec<MissingField>,
}

/// 缺失扫描中的单文件错误。错误文件不会被伪造为命中或已补全。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingScanError {
    pub path: String,
    pub reason: String,
}

/// 缺失字段扫描结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MissingScanResult {
    pub songs: Vec<MissingSong>,
    pub errors: Vec<MissingScanError>,
}

/// 歌词来源（design.md D1 契约形状：`"embedded" | "sidecar" | "none"`）。
///
/// serde 默认会把 enum 序列化成 `{"Embedded":null}` 对象形状，破坏 TS 契约，
/// 必须逐变体 rename 对齐 `src/api/types.ts` 的字面量。`SidecarLrc` 若只依赖
/// `rename_all = "snake_case"` 会得到 `"sidecar_lrc"`，故显式 `rename = "sidecar"`。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LyricsSource {
    Embedded,
    #[serde(rename = "sidecar")]
    SidecarLrc,
    None,
}

/// 完整标签（open_song 返回 / save_song 提交）。字段与 TS `Song` 契约逐项对齐。
///
/// - 文本字段一律 `String`，未设置读空串（PRD §6），Rust 侧不 trim。
/// - `cover` 为 base64 data URL（`data:<mime>;base64,...`），前端 `<img :src>` 直接用。
/// - `lyrics_source` 本变更只落内嵌判定（trim 非空 → Embedded，否则 None）；
///   `SidecarLrc` 由 v1-lyrics-lrc 补充。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Song {
    pub path: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub album_artist: String,
    pub track: String,
    pub track_total: String,
    pub year: String,
    pub genre: String,
    pub lyrics: String,
    pub lyrics_source: LyricsSource,
    pub cover: Option<String>,
    pub cover_mime: Option<String>,
}

/// 封面选择/拖拽输入（`pick_cover_file` / `read_cover_path` 返回，v1-cover-embed）。
///
/// 字段与 TS `CoverInput` 契约逐字对齐（`src/api/types.ts`）：
/// - `data_url`：压缩后小图的 base64 data URL（`data:<mime>;base64,...`），
///   直接进 `Song.cover`（`<img :src>` 同形状，前端零转换，design.md §10.3 / D1）；
/// - `mime`：`image/jpeg | image/png | image/webp`，供 `cover_mime` 展示。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoverInput {
    pub data_url: String,
    pub mime: String,
}

/// 搜索来源（design.md §10.3 契约形状：`"netease" | "qqmusic" | "kugou" | "lrclib" | "itunes"`，
/// v1-search-backend / search-sources-renewal 五源）。
///
/// serde 默认会把 enum 序列化成 `{"Netease":null}` 对象形状，且 `rename_all = "snake_case"`
/// 会把 `QqMusic` 序列化成 `"qq_music"`（≠ 前端 TS 字面量 `'qqmusic'`），必须显式
/// `#[serde(rename = "qqmusic")]` —— 与 `LyricsSource::SidecarLrc` 显式 `rename = "sidecar"` 同款教训。
/// `Kugou`/`Lrclib`/`Itunes` 在 snake_case 下自动映射 `"kugou"|"lrclib"|"itunes"`，无需显式 rename。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MusicSourceId {
    Netease,
    #[serde(rename = "qqmusic")]
    QqMusic,
    Kugou,
    Lrclib,
    Itunes,
}

/// 搜索候选（design.md §10.3 契约，v1-search-backend）。
///
/// 惰性拉取：候选只带 `id` + `cover_url`（随三家搜索响应带出），歌词文本 / 封面字节
/// 点选后才 `fetch_lyric` / `download_cover`。无 `year` 字段（§10.3 TS 未定义）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SongCandidate {
    pub source: MusicSourceId,
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub cover_url: Option<String>,
}

/// 五源并发搜索结果（design.md §10.3 契约，v1-search-backend / v1-search-fixes / search-sources-renewal）。
///
/// `source_stats` 元组序列化为 `[source, count]` 数组，对齐 TS
/// `Array<[MusicSourceId, number]>`（各家成功返回的候选条数，失败/超时记 0）。
/// `all_failed`：五源**全部失败**（网络错误/超时）→ true；至少一源成功（含正常空结果）→
/// false。前端仅在 `all_failed` 时判定会话离线（FR-8.4a「全部源失败」——冷门歌正常空结果不标离线）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResult {
    pub songs: Vec<SongCandidate>,
    pub source_stats: Vec<(MusicSourceId, usize)>,
    pub all_failed: bool,
}

/// 批量修改标签字段（Option::Some 表示需要修改，None 表示保持原样）
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BatchTagFields {
    pub album: Option<String>,
    pub artist: Option<String>,
    pub album_artist: Option<String>,
    pub year: Option<String>,
    pub genre: Option<String>,
    pub track_total: Option<String>,
    pub lyrics: Option<String>,
    pub cover: Option<String>,
    pub titles: Option<std::collections::HashMap<String, String>>,
    pub tracks: Option<std::collections::HashMap<String, String>>,
    pub rename_files: Option<std::collections::HashMap<String, String>>,
}

/// 批量修改执行结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchUpdateResult {
    pub total: usize,
    pub success_count: usize,
    pub failure_count: usize,
    pub errors: Vec<String>,
}

fn default_true() -> bool {
    true
}

fn default_min_similarity() -> f32 {
    0.60
}

fn default_concurrency() -> usize {
    4
}

/// 批量自动匹配刮削配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMatchOptions {
    #[serde(default)]
    pub sources: Vec<MusicSourceId>,
    #[serde(default = "default_true")]
    pub overwrite_title: bool,
    #[serde(default = "default_true")]
    pub overwrite_artist: bool,
    #[serde(default = "default_true")]
    pub overwrite_album: bool,
    #[serde(default = "default_true")]
    pub overwrite_cover: bool,
    #[serde(default = "default_true")]
    pub overwrite_lyrics: bool,
    #[serde(default)]
    pub only_missing: bool,
    #[serde(default)]
    pub export_lrc: bool,
    #[serde(default = "default_min_similarity")]
    pub min_similarity: f32,
    #[serde(default = "default_concurrency")]
    pub concurrency: usize,
}

impl Default for BatchMatchOptions {
    fn default() -> Self {
        Self {
            sources: vec![
                MusicSourceId::Netease,
                MusicSourceId::QqMusic,
                MusicSourceId::Kugou,
                MusicSourceId::Lrclib,
                MusicSourceId::Itunes,
            ],
            overwrite_title: true,
            overwrite_artist: true,
            overwrite_album: true,
            overwrite_cover: true,
            overwrite_lyrics: true,
            only_missing: false,
            export_lrc: false,
            min_similarity: 0.60,
            concurrency: 4,
        }
    }
}

/// 批量自动匹配单曲处理进度事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMatchProgress {
    pub current: usize,
    pub total: usize,
    pub current_path: String,
    pub current_title: String,
    pub status: String, // "matched" | "skipped" | "failed"
    pub matched_source: Option<MusicSourceId>,
    pub matched_title: Option<String>,
    pub matched_artist: Option<String>,
    pub similarity: Option<f32>,
    pub error: Option<String>,
}

/// 批量自动匹配最终汇总结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchMatchSummary {
    pub total: usize,
    pub matched_count: usize,
    pub skipped_count: usize,
    pub failed_count: usize,
    pub logs: Vec<BatchMatchProgress>,
}
