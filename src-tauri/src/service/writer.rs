// MusicTag — 保存编排（probe → primary_tag_mut().clear() → apply_meta → write_atomic）。
//
// 语义（PRD FR-5.5）：`Song` 中的非空字段写入标签，空字段被清除；
// `cover=None` 即删除封面；写回**原路径**，不产生新文件、不改文件名。
// 函数提升 `pub` 供 `src-tauri/tests/` 集成测试经 `app_lib::service::writer::` 访问。

use crate::model::Song;
use crate::service::fs_atomic::write_atomic_with;
use crate::service::meta::{apply_meta, apply_riff_info_meta};
use lofty::ape::ApeTag;
use lofty::config::WriteOptions;
use lofty::file::{AudioFile, FileType};
use lofty::prelude::{TagExt, TaggedFileExt};
use lofty::probe::Probe;
use lofty::tag::Tag;
use lofty::tag::TagType;
use std::path::Path;

/// 保存当前编辑表单，全量覆盖写回原路径。
///
/// 写盘策略（D6）：读入 → `clear()` 重建标签 → 写同目录临时文件 →
/// rename 原子替换原路径。任一环节失败返回 `Err(String)`，原文件不被触碰。
///
/// `export_lrc`（design.md D3/D4）：复选框 opt-in 同步写 `.lrc`。写顺序**内嵌先、
/// `.lrc` 后**——内嵌是主存储（FR-4.1）；`.lrc` 失败 `?` 并入总 Err（前缀「写 .lrc
/// 失败:」），绝不吞错报成功（`.lrc` 为空时 `export_lrc` 内部 no-op，FR-4.4a）。
pub fn save_song(song: Song, export_lrc: bool) -> Result<(), String> {
    let path = Path::new(&song.path);

    // 写前校验格式（PRD「稳健」）：格式损坏/不可读 → Err，原文件未动。
    let mut tagged_file = Probe::open(path)
        .and_then(|probed| probed.read())
        .map_err(|e| format!("读取标签失败: {e}"))?;

    // lofty 仅在 RIFF INFO 非空时暴露该辅助标签。记下其原始存在性，避免为
    // ID3v2-only WAV 新建 RIFF INFO，同时使原有 INFO 的旧值可随表单被清除。
    let has_riff_info =
        tagged_file.file_type() == FileType::Wav && tagged_file.tag(TagType::RiffInfo).is_some();

    // D2：primary tag `clear()` 重建，保证「最终标签 == 表单内容」。
    // 无标签但格式可写时先创建 lofty 为该容器选择的 primary tag。
    if tagged_file.primary_tag().is_none() {
        let tag_type = tagged_file.primary_tag_type();
        if !matches!(tag_type, TagType::Ape | TagType::Id3v2 | TagType::Mp4Ilst) {
            return Err("读取标签失败: 文件缺少可写的主标签".to_string());
        }
        tagged_file.insert_tag(Tag::new(tag_type));
    }
    {
        let tag = tagged_file
            .primary_tag_mut()
            .ok_or_else(|| "读取标签失败: 文件缺少可写的主标签".to_string())?;
        tag.clear();
        apply_meta(tag, &song)?;
    }

    if has_riff_info {
        let riff_info = tagged_file
            .tag_mut(TagType::RiffInfo)
            .expect("已确认存在的 RIFF INFO 标签应仍可访问");
        riff_info.clear();
        apply_riff_info_meta(riff_info, &song);
    }

    let ape_tag = tagged_file
        .primary_tag()
        .filter(|tag| tag.tag_type() == TagType::Ape)
        .map(|tag| ApeTag::from(tag.clone()));

    write_atomic_with(path, move |file| {
        if let Some(ape_tag) = ape_tag {
            use lofty::prelude::TagExt;
            ape_tag
                .save_to(file, WriteOptions::default())
                .map_err(std::io::Error::other)?;
        } else {
            tagged_file
                .save_to(file, WriteOptions::default())
                .map_err(std::io::Error::other)?;
        }
        Ok(())
    })
    .map_err(|e| format!("写回文件失败: {e}"))?;

    if export_lrc {
        crate::service::lyrics::export_lrc(path, &song.lyrics)
            .map_err(|e| format!("写 .lrc 失败: {e}"))?;
    }
    Ok(())
}

/// 批量修改多个音频文件的指定标签字段（播客/音频批量改专辑名等）。
/// 仅修改 fields 中为 Some 的字段，其余字段完全保留原样。
pub fn batch_update_tags(
    paths: &[String],
    fields: &crate::model::BatchTagFields,
) -> Result<crate::model::BatchUpdateResult, String> {
    let mut success_count = 0;
    let mut failure_count = 0;
    let mut errors = Vec::new();

    for p in paths {
        let path = Path::new(p);
        if !path.exists() {
            errors.push(format!("{p}: 文件不存在"));
            failure_count += 1;
            continue;
        }

        match update_single_file_tags(path, fields) {
            Ok(_) => success_count += 1,
            Err(e) => {
                errors.push(format!("{p}: {e}"));
                failure_count += 1;
            }
        }
    }

    Ok(crate::model::BatchUpdateResult {
        total: paths.len(),
        success_count,
        failure_count,
        errors,
    })
}

fn update_single_file_tags(
    path: &Path,
    fields: &crate::model::BatchTagFields,
) -> Result<(), String> {
    let mut tagged_file = Probe::open(path)
        .and_then(|probed| probed.read())
        .map_err(|e| format!("读取标签失败: {e}"))?;

    let has_riff_info =
        tagged_file.file_type() == FileType::Wav && tagged_file.tag(TagType::RiffInfo).is_some();

    if tagged_file.primary_tag().is_none() {
        let tag_type = tagged_file.primary_tag_type();
        if !matches!(tag_type, TagType::Ape | TagType::Id3v2 | TagType::Mp4Ilst) {
            return Err("文件缺少可写的主标签".to_string());
        }
        tagged_file.insert_tag(Tag::new(tag_type));
    }

    {
        use lofty::tag::ItemKey;
        let tag = tagged_file
            .primary_tag_mut()
            .ok_or_else(|| "文件缺少可写的主标签".to_string())?;

        let path_str = path.to_string_lossy();

        if let Some(ref titles) = fields.titles {
            if let Some(title) = titles.get(path_str.as_ref()) {
                apply_field(tag, ItemKey::TrackTitle, title);
            }
        }
        if let Some(ref tracks) = fields.tracks {
            if let Some(track) = tracks.get(path_str.as_ref()) {
                apply_field(tag, ItemKey::TrackNumber, track);
            }
        }
        if let Some(ref track_total) = fields.track_total {
            apply_field(tag, ItemKey::TrackTotal, track_total);
        }
        if let Some(ref album) = fields.album {
            apply_field(tag, ItemKey::AlbumTitle, album);
        }
        if let Some(ref artist) = fields.artist {
            apply_field(tag, ItemKey::TrackArtist, artist);
        }
        if let Some(ref album_artist) = fields.album_artist {
            apply_field(tag, ItemKey::AlbumArtist, album_artist);
        }
        if let Some(ref year) = fields.year {
            apply_field(tag, ItemKey::RecordingDate, year);
        }
        if let Some(ref genre) = fields.genre {
            apply_field(tag, ItemKey::Genre, genre);
        }
        if let Some(ref lyrics) = fields.lyrics {
            apply_field(tag, ItemKey::Lyrics, lyrics);
            apply_field(tag, ItemKey::UnsyncLyrics, lyrics);
        }
        if let Some(ref cover_str) = fields.cover {
            while !tag.pictures().is_empty() {
                tag.remove_picture(0);
            }
            if !cover_str.trim().is_empty() {
                let (bytes, mime) = crate::service::cover::decode_cover(cover_str)?;
                let (comp_bytes, comp_mime) = crate::service::cover::compress_cover(&bytes, &mime)
                    .unwrap_or((bytes, mime));
                let pic = lofty::picture::Picture::unchecked(comp_bytes)
                    .pic_type(lofty::picture::PictureType::CoverFront)
                    .mime_type(lofty::picture::MimeType::from_str(&comp_mime))
                    .build();
                tag.push_picture(pic);
            }
        }
    }

    if has_riff_info {
        use lofty::tag::ItemKey;
        if let Some(riff_info) = tagged_file.tag_mut(TagType::RiffInfo) {
            let path_str = path.to_string_lossy();
            if let Some(ref titles) = fields.titles {
                if let Some(title) = titles.get(path_str.as_ref()) {
                    apply_field(riff_info, ItemKey::TrackTitle, title);
                }
            }
            if let Some(ref tracks) = fields.tracks {
                if let Some(track) = tracks.get(path_str.as_ref()) {
                    apply_field(riff_info, ItemKey::TrackNumber, track);
                }
            }
            if let Some(ref track_total) = fields.track_total {
                apply_field(riff_info, ItemKey::TrackTotal, track_total);
            }
            if let Some(ref album) = fields.album {
                apply_field(riff_info, ItemKey::AlbumTitle, album);
            }
            if let Some(ref artist) = fields.artist {
                apply_field(riff_info, ItemKey::TrackArtist, artist);
            }
            if let Some(ref year) = fields.year {
                apply_field(riff_info, ItemKey::RecordingDate, year);
            }
            if let Some(ref genre) = fields.genre {
                apply_field(riff_info, ItemKey::Genre, genre);
            }
        }
    }

    let ape_tag = tagged_file
        .primary_tag()
        .filter(|tag| tag.tag_type() == TagType::Ape)
        .map(|tag| ApeTag::from(tag.clone()));

    write_atomic_with(path, move |file| {
        if let Some(ape_tag) = ape_tag {
            use lofty::prelude::TagExt;
            ape_tag
                .save_to(file, WriteOptions::default())
                .map_err(std::io::Error::other)?;
        } else {
            tagged_file
                .save_to(file, WriteOptions::default())
                .map_err(std::io::Error::other)?;
        }
        Ok(())
    })
    .map_err(|e| format!("写回文件失败: {e}"))?;

    if let Some(ref rename_map) = fields.rename_files {
        let path_str = path.to_string_lossy();
        if let Some(new_name) = rename_map.get(path_str.as_ref()) {
            if let Some(old_name) = path.file_name().and_then(|s| s.to_str()) {
                if old_name != new_name {
                    crate::service::rename::rename_song(path, new_name)?;
                }
            }
        }
    }

    Ok(())
}

fn apply_field(tag: &mut Tag, key: lofty::tag::ItemKey, val: &str) {
    if val.is_empty() {
        tag.remove_key(key);
    } else {
        tag.insert_text(key, val.to_string());
    }
}
