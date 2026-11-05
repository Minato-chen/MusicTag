// MusicTag — 批量自动匹配与刮削（逆向 Fkosoft MusicTag 核心启发式算法）
//
// 核心逻辑：
// 1. 标题与艺术家归一化（繁简折叠、全角转半角、小写、trim）
// 2. 归一化 Levenshtein 距离与多艺人拆分打分（阈值 0.60，与 MusicTag.exe 对齐）
// 3. 文件名回退解析（无标签裸文件自动提取「艺人 - 歌名」）
// 4. 五源并发检索 + 限流信号量并发控制（默认 4-8 并发）
// 5. 惰性拉取歌词与封面，并原子写盘回写
// 6. 实时进度事件推送（`batch-match-progress`）

use crate::model::{
    BatchMatchOptions, BatchMatchProgress, BatchMatchSummary, LyricsSource, MusicSourceId,
    SongCandidate,
};
use crate::service::reader;
use crate::service::searcher::{
    itunes::Itunes, kugou::Kugou, lrclib::Lrclib, netease::Netease, norm, qqmusic::QqMusic,
    search_song_with_sources, MusicSource,
};
use crate::service::writer;
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// 计算两个字符串的 Levenshtein 编辑距离
pub fn levenshtein_distance(s1: &str, s2: &str) -> usize {
    let s1_chars: Vec<char> = s1.chars().collect();
    let s2_chars: Vec<char> = s2.chars().collect();
    let (len1, len2) = (s1_chars.len(), s2_chars.len());
    if len1 == 0 {
        return len2;
    }
    if len2 == 0 {
        return len1;
    }

    let mut prev_row: Vec<usize> = (0..=len2).collect();
    let mut curr_row: Vec<usize> = vec![0; len2 + 1];

    for (i, &c1) in s1_chars.iter().enumerate() {
        curr_row[0] = i + 1;
        for (j, &c2) in s2_chars.iter().enumerate() {
            let cost = if c1 == c2 { 0 } else { 1 };
            curr_row[j + 1] = (prev_row[j + 1] + 1)
                .min(curr_row[j] + 1)
                .min(prev_row[j] + cost);
        }
        prev_row.copy_from_slice(&curr_row);
    }
    prev_row[len2]
}

/// 归一化编辑距离相似度：1.0 - (dist / max_len)，区间 [0.0, 1.0]
pub fn normalized_levenshtein(s1: &str, s2: &str) -> f32 {
    let s1 = s1.trim();
    let s2 = s2.trim();
    if s1.is_empty() && s2.is_empty() {
        return 1.0;
    }
    let max_len = s1.chars().count().max(s2.chars().count());
    if max_len == 0 {
        return 1.0;
    }
    let dist = levenshtein_distance(s1, s2);
    (1.0 - (dist as f32 / max_len as f32)).max(0.0)
}

/// 综合字符串相似度（包含子串包含奖励，如 Live 版 / 伴奏等标题变体）
pub fn string_similarity(s1: &str, s2: &str) -> f32 {
    let s1 = s1.trim();
    let s2 = s2.trim();
    if s1.is_empty() && s2.is_empty() {
        return 1.0;
    }
    if s1.is_empty() || s2.is_empty() {
        return 0.0;
    }
    if s1 == s2 {
        return 1.0;
    }
    let lev_sim = normalized_levenshtein(s1, s2);
    // 子串包含加权（例如 "晴天" 与 "晴天 (Live)"）
    if s1.contains(s2) || s2.contains(s1) {
        let shorter = s1.chars().count().min(s2.chars().count()) as f32;
        let longer = s1.chars().count().max(s2.chars().count()) as f32;
        let ratio = shorter / longer;
        let contain_sim = 0.70 + 0.30 * ratio;
        lev_sim.max(contain_sim)
    } else {
        lev_sim
    }
}

/// 多艺人拆分匹配（以 `/`, `,`, `&`, `、`, `，`, `+` 分隔）
pub fn multi_artist_similarity(q: &str, c: &str) -> f32 {
    if q == c {
        return 1.0;
    }
    if q.contains(c) || c.contains(q) {
        return 0.9;
    }
    let c_artists: Vec<&str> = c
        .split(|ch: char| matches!(ch, '/' | ',' | '&' | '、' | '，' | '+'))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();
    let q_artists: Vec<&str> = q
        .split(|ch: char| matches!(ch, '/' | ',' | '&' | '、' | '，' | '+'))
        .map(|s| s.trim())
        .filter(|s| !s.is_empty())
        .collect();

    let mut max_sim: f32 = 0.0;
    for qa in &q_artists {
        for ca in &c_artists {
            if qa == ca {
                return 1.0;
            }
            if qa.contains(ca) || ca.contains(qa) {
                max_sim = max_sim.max(0.85);
            }
            let sim = string_similarity(qa, ca);
            max_sim = max_sim.max(sim);
        }
    }
    max_sim
}

/// 候选打分（MusicTag.exe 启发式权重：标题 65%，艺人 35%）
pub fn score_candidate(query_title: &str, query_artist: &str, cand: &SongCandidate) -> f32 {
    let norm_q_title = norm(query_title);
    let norm_c_title = norm(&cand.title);
    let title_sim = string_similarity(&norm_q_title, &norm_c_title);

    let norm_q_artist = norm(query_artist);
    let norm_c_artist = norm(&cand.artist);

    if norm_q_artist.is_empty() {
        // 无原艺人信息，纯依赖歌名相似度
        title_sim
    } else if norm_c_artist.is_empty() {
        title_sim * 0.8
    } else {
        let artist_sim = multi_artist_similarity(&norm_q_artist, &norm_c_artist);
        0.65 * title_sim + 0.35 * artist_sim
    }
}

/// 从文件名剥离序号前缀（如 "01. "、"01 - "、"01 " 等）
fn strip_track_prefix(s: &str) -> &str {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() && bytes[i].is_ascii_digit() {
        i += 1;
    }
    if i > 0 && i < bytes.len() {
        let rest = &s[i..];
        if rest.starts_with(". ") || rest.starts_with("- ") {
            return rest[2..].trim();
        } else if rest.starts_with('.') || rest.starts_with('-') || rest.starts_with(' ') {
            return rest[1..].trim();
        }
    }
    s
}

/// 从文件名回退解析艺人和歌名
pub fn parse_filename_fallback(path_str: &str) -> (String, String) {
    let stem = Path::new(path_str)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .trim();

    if stem.is_empty() {
        return (String::new(), String::new());
    }

    let clean_stem = strip_track_prefix(stem);

    if let Some((part1, part2)) = clean_stem.split_once(" - ") {
        let p1 = part1.trim().to_string();
        let p2 = part2.trim().to_string();
        if !p1.is_empty() && !p2.is_empty() {
            return (p1, p2);
        }
    }

    (String::new(), clean_stem.to_string())
}

/// 根据配置选择检索源列表
fn get_sources(selected: &[MusicSourceId]) -> Vec<Box<dyn MusicSource>> {
    let mut sources: Vec<Box<dyn MusicSource>> = Vec::new();
    let sel = if selected.is_empty() {
        &[
            MusicSourceId::Netease,
            MusicSourceId::QqMusic,
            MusicSourceId::Kugou,
            MusicSourceId::Lrclib,
            MusicSourceId::Itunes,
        ][..]
    } else {
        selected
    };
    for src in sel {
        match src {
            MusicSourceId::Netease => sources.push(Box::new(Netease::default())),
            MusicSourceId::QqMusic => sources.push(Box::new(QqMusic::default())),
            MusicSourceId::Kugou => sources.push(Box::new(Kugou::default())),
            MusicSourceId::Lrclib => sources.push(Box::new(Lrclib::default())),
            MusicSourceId::Itunes => sources.push(Box::new(Itunes::default())),
        }
    }
    sources
}

/// 对单首歌曲执行自动匹配与保存
async fn match_single_song(
    path_str: &str,
    options: &BatchMatchOptions,
) -> Result<(String, Option<SongCandidate>, Option<f32>), String> {
    let path = Path::new(path_str);
    let mut song = reader::read_song_meta(path)?;

    let (file_artist, file_title) = parse_filename_fallback(path_str);
    let query_title = if !song.title.trim().is_empty() {
        song.title.clone()
    } else {
        file_title
    };
    let query_artist = if !song.artist.trim().is_empty() {
        song.artist.clone()
    } else {
        file_artist
    };

    if query_title.trim().is_empty() {
        return Ok(("skipped".to_string(), None, None));
    }

    let client = crate::service::searcher::client();
    let sources = get_sources(&options.sources);
    let search_result = search_song_with_sources(
        client,
        &query_title,
        &query_artist,
        &song.album,
        sources,
        Duration::from_secs(6),
    )
    .await;

    if search_result.songs.is_empty() {
        return Ok(("skipped".to_string(), None, None));
    }

    // 候选评分排序
    let mut scored: Vec<(f32, SongCandidate)> = search_result
        .songs
        .into_iter()
        .map(|cand| {
            let score = score_candidate(&query_title, &query_artist, &cand);
            (score, cand)
        })
        .collect();

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

    let (best_score, best_cand) = scored.remove(0);
    if best_score < options.min_similarity {
        return Ok(("skipped".to_string(), None, Some(best_score)));
    }

    let candidate = best_cand;

    // 按配置覆盖字段
    if options.overwrite_title && (!options.only_missing || song.title.trim().is_empty()) {
        song.title = candidate.title.clone();
    }
    if options.overwrite_artist && (!options.only_missing || song.artist.trim().is_empty()) {
        song.artist = candidate.artist.clone();
    }
    if options.overwrite_album && (!options.only_missing || song.album.trim().is_empty()) {
        if !candidate.album.trim().is_empty() {
            song.album = candidate.album.clone();
        }
    }

    // 拉取并写入歌词
    if options.overwrite_lyrics && (!options.only_missing || song.lyrics.trim().is_empty()) {
        if let Some(lyrics) =
            crate::service::searcher::fetch_lyric(client, candidate.source, &candidate.id).await
        {
            if !lyrics.trim().is_empty() {
                song.lyrics = lyrics;
                song.lyrics_source = LyricsSource::Embedded;
            }
        }
    }

    // 拉取并写入封面
    if options.overwrite_cover && (!options.only_missing || song.cover.is_none()) {
        if let Some(ref cover_url) = candidate.cover_url {
            if let Ok(bytes) = crate::service::searcher::download_cover(client, cover_url).await {
                let mime = image::guess_format(&bytes)
                    .map(|f| f.to_mime_type().to_string())
                    .unwrap_or_else(|_| "image/jpeg".to_string());
                let (comp_bytes, comp_mime) = crate::service::cover::compress_cover(&bytes, &mime)
                    .unwrap_or((bytes, mime));
                let b64 = BASE64.encode(&comp_bytes);
                song.cover = Some(format!("data:{comp_mime};base64,{b64}"));
                song.cover_mime = Some(comp_mime);
            }
        }
    }

    // 原子覆盖写盘
    writer::save_song(song, options.export_lrc)?;

    Ok(("matched".to_string(), Some(candidate), Some(best_score)))
}

/// 批量自动匹配入口函数
pub async fn batch_auto_match(
    paths: &[String],
    options: &BatchMatchOptions,
    app: Option<&tauri::AppHandle>,
) -> Result<BatchMatchSummary, String> {
    if paths.is_empty() {
        return Ok(BatchMatchSummary {
            total: 0,
            matched_count: 0,
            skipped_count: 0,
            failed_count: 0,
            logs: Vec::new(),
        });
    }

    let concurrency = options.concurrency.clamp(1, 8);
    let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency));
    let total = paths.len();
    let current_index = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let mut tasks = tokio::task::JoinSet::new();

    for path in paths {
        let path = path.clone();
        let options = options.clone();
        let sem = semaphore.clone();
        let current_index = current_index.clone();
        let app = app.cloned();

        tasks.spawn(async move {
            let _permit = sem.acquire().await.map_err(|e| e.to_string());
            let idx = current_index.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
            let result = match_single_song(&path, &options).await;
            let progress = match result {
                Ok((status, matched_cand, similarity)) => {
                    let (matched_source, matched_title, matched_artist) = match matched_cand {
                        Some(c) => (Some(c.source), Some(c.title), Some(c.artist)),
                        None => (None, None, None),
                    };
                    BatchMatchProgress {
                        current: idx,
                        total,
                        current_path: path.clone(),
                        current_title: matched_title.clone().unwrap_or_else(|| path.clone()),
                        status,
                        matched_source,
                        matched_title,
                        matched_artist,
                        similarity,
                        error: None,
                    }
                }
                Err(e) => BatchMatchProgress {
                    current: idx,
                    total,
                    current_path: path.clone(),
                    current_title: path.clone(),
                    status: "failed".to_string(),
                    matched_source: None,
                    matched_title: None,
                    matched_artist: None,
                    similarity: None,
                    error: Some(e),
                },
            };

            if let Some(ref app) = app {
                use tauri::Emitter;
                let _ = app.emit("batch-match-progress", &progress);
            }

            progress
        });
    }

    let mut logs = Vec::new();
    let mut matched_count = 0;
    let mut skipped_count = 0;
    let mut failed_count = 0;

    while let Some(res) = tasks.join_next().await {
        if let Ok(prog) = res {
            match prog.status.as_str() {
                "matched" => matched_count += 1,
                "skipped" => skipped_count += 1,
                _ => failed_count += 1,
            }
            logs.push(prog);
        }
    }

    logs.sort_by_key(|l| l.current);

    Ok(BatchMatchSummary {
        total,
        matched_count,
        skipped_count,
        failed_count,
        logs,
    })
}
