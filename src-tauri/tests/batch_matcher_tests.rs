use app_lib::model::{BatchMatchOptions, MusicSourceId, SongCandidate};
use app_lib::service::batch_matcher::{
    batch_auto_match, levenshtein_distance, multi_artist_similarity, normalized_levenshtein,
    parse_filename_fallback, score_candidate, string_similarity,
};

#[test]
fn test_levenshtein_distance() {
    assert_eq!(levenshtein_distance("", ""), 0);
    assert_eq!(levenshtein_distance("晴天", "晴天"), 0);
    assert_eq!(levenshtein_distance("晴天", "阴天"), 1);
    assert_eq!(levenshtein_distance("晴天", "七里香"), 3);
}

#[test]
fn test_normalized_levenshtein() {
    assert!((normalized_levenshtein("晴天", "晴天") - 1.0).abs() < f32::EPSILON);
    assert!((normalized_levenshtein("晴天", "阴天") - 0.5).abs() < f32::EPSILON);
    assert!((normalized_levenshtein("", "") - 1.0).abs() < f32::EPSILON);
}

#[test]
fn test_string_similarity_containment() {
    // 包含关系加权：Live 版或伴奏等
    let sim = string_similarity("晴天", "晴天 (Live)");
    assert!(sim >= 0.70, "sim was {sim}");

    let diff = string_similarity("晴天", "暗号");
    assert!(diff < 0.1, "diff was {diff}");
}

#[test]
fn test_multi_artist_similarity() {
    // 单艺人完全匹配
    assert!((multi_artist_similarity("周杰伦", "周杰伦") - 1.0).abs() < f32::EPSILON);

    // 多艺人拆分匹配
    let sim1 = multi_artist_similarity("周杰伦", "周杰伦 / 蔡依林");
    assert!(sim1 >= 0.90, "sim1 was {sim1}");

    let sim2 = multi_artist_similarity("林俊杰", "林俊杰, 阿信");
    assert!(sim2 >= 0.90, "sim2 was {sim2}");

    // 完全无关艺人
    let sim_none = multi_artist_similarity("周杰伦", "陈奕迅");
    assert!(sim_none < 0.40, "sim_none was {sim_none}");
}

#[test]
fn test_score_candidate() {
    let cand = SongCandidate {
        source: MusicSourceId::Netease,
        id: "123".to_string(),
        title: "晴天".to_string(),
        artist: "周杰伦".to_string(),
        album: "叶惠美".to_string(),
        cover_url: None,
    };

    // 精确匹配 -> 得分应接近 1.0
    let score = score_candidate("晴天", "周杰伦", &cand);
    assert!(score > 0.95, "score was {score}");

    // 繁体原词匹配（经 norm 繁简折叠后应高分命中）
    let score_trad = score_candidate("晴天", "周杰倫", &cand);
    assert!(score_trad > 0.95, "score_trad was {score_trad}");

    // 标题相同但翻唱 -> 仍有较高歌名分，但低于原唱
    let cand_cover = SongCandidate {
        source: MusicSourceId::QqMusic,
        id: "456".to_string(),
        title: "晴天".to_string(),
        artist: "某翻唱歌手".to_string(),
        album: "翻唱合集".to_string(),
        cover_url: None,
    };
    let score_cover = score_candidate("晴天", "周杰伦", &cand_cover);
    assert!(score > score_cover);
}

#[test]
fn test_parse_filename_fallback() {
    // 经典 "艺人 - 歌名"
    let (artist, title) = parse_filename_fallback("/music/周杰伦 - 晴天.mp3");
    assert_eq!(artist, "周杰伦");
    assert_eq!(title, "晴天");

    // 带序号前缀 "01. 周杰伦 - 晴天"
    let (artist2, title2) = parse_filename_fallback("/music/01. 周杰伦 - 晴天.flac");
    assert_eq!(artist2, "周杰伦");
    assert_eq!(title2, "晴天");

    // 仅歌名 "晴天.wav"
    let (artist3, title3) = parse_filename_fallback("/music/晴天.wav");
    assert_eq!(artist3, "");
    assert_eq!(title3, "晴天");
}

#[tokio::test]
async fn test_batch_auto_match_empty() {
    let options = BatchMatchOptions::default();
    let res = batch_auto_match(&[], &options, None).await.unwrap();
    assert_eq!(res.total, 0);
    assert_eq!(res.matched_count, 0);
}
