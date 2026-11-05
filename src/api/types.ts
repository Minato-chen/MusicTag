// Tauri command 契约类型（design.md §10，与 Rust model.rs struct 对齐）。
// Rust enum 映射：LyricsSource Embedded/SidecarLrc/None ↔ 'embedded'|'sidecar'|'none'
//                MusicSourceId Netease/QqMusic/Kugou/Lrclib/Itunes ↔ 'netease'|'qqmusic'|'kugou'|'lrclib'|'itunes'

/** 音乐来源 ID：网易云 / QQ / 酷狗 / LRCLIB / iTunes */
export type MusicSourceId = 'netease' | 'qqmusic' | 'kugou' | 'lrclib' | 'itunes'

/** 歌词来源：内嵌 / 同名单曲 lrc / 无 */
export type LyricsSource = 'embedded' | 'sidecar' | 'none'

/** 查漏扫描维度（与 Rust MissingField 的 snake_case serde 名称一致）。 */
export type MissingField = 'title' | 'artist' | 'album' | 'cover' | 'lyrics'

export interface MissingSong {
  path: string
  missing: MissingField[]
}

export interface MissingScanError {
  path: string
  reason: string
}

export interface MissingScanResult {
  songs: MissingSong[]
  errors: MissingScanError[]
}

/** 与 Rust Song struct 对齐（open_song 返回 / save_song 提交）。 */
export interface Song {
  path: string
  title: string
  artist: string
  album: string
  album_artist: string
  track: string
  track_total: string
  year: string
  genre: string
  lyrics: string
  lyrics_source: LyricsSource
  cover: string | null // base64 data URL（data:image/jpeg;base64,...）
  cover_mime: string | null
}

/** 列表轻量项（list_songs 返回；详情选中后再 open_song 读）。 */
export interface SongSummary {
  path: string
  title: string
  artist: string
}

/** 封面选择/拖拽输入（pick_cover_file / read_cover_path 返回，与 Rust CoverInput 对齐）。
 *  data_url 直接进 `Song.cover`（`<img :src>` 同形状，前端零转换）；mime 供 cover_mime 展示。 */
export interface CoverInput {
  data_url: string // base64 data URL（data:<mime>;base64,...，压缩后小图）
  mime: string // image/jpeg | image/png | image/webp
}

/** 搜索候选（search_song 返回）。 */
export interface SongCandidate {
  source: MusicSourceId
  id: string
  title: string
  artist: string
  album: string
  cover_url: string | null
}

/** 五源聚合搜索结果。 */
export interface SearchResult {
  songs: SongCandidate[]
  source_stats: Array<[MusicSourceId, number]> // 各家返回条数
  /** 五源全部失败（网络错误/超时）→ true；至少一源成功（含正常空结果）→ false。前端仅在 true 时判定会话离线（FR-8.4a）。 */
  all_failed: boolean
}

/** 当前目录失效通知；与 Rust FolderChanged 的 camelCase 字段一致。 */
export interface FolderChanged {
  dir: string
  watchId: number
  error: string | null
}

/** 批量修改标签字段（undefined 表示不修改，留空/非空表示更新） */
export interface BatchTagFields {
  album?: string
  artist?: string
  album_artist?: string
  year?: string
  genre?: string
  track_total?: string
  lyrics?: string
  cover?: string
  titles?: Record<string, string>
  tracks?: Record<string, string>
  rename_files?: Record<string, string>
}

/** 批量修改执行结果 */
export interface BatchUpdateResult {
  total: number
  success_count: number
  failure_count: number
  errors: string[]
}

/** 批量自动匹配配置 */
export interface BatchMatchOptions {
  sources?: MusicSourceId[]
  overwrite_title?: boolean
  overwrite_artist?: boolean
  overwrite_album?: boolean
  overwrite_cover?: boolean
  overwrite_lyrics?: boolean
  only_missing?: boolean
  export_lrc?: boolean
  min_similarity?: number
  concurrency?: number
}

/** 批量自动匹配单曲处理进度事件 */
export interface BatchMatchProgress {
  current: number
  total: number
  current_path: string
  current_title: string
  status: 'matched' | 'skipped' | 'failed' | string
  matched_source: MusicSourceId | null
  matched_title: string | null
  matched_artist: string | null
  similarity: number | null
  error: string | null
}

/** 批量自动匹配最终汇总结果 */
export interface BatchMatchSummary {
  total: number
  matched_count: number
  skipped_count: number
  failed_count: number
  logs: BatchMatchProgress[]
}

