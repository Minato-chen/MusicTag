<script setup lang="ts">
import { computed, onUnmounted, ref } from 'vue'
import { batchAutoMatch, listenBatchMatchProgress } from '../api/songs'
import type { BatchMatchOptions, BatchMatchProgress, BatchMatchSummary, MusicSourceId } from '../api/types'
import { fileName } from '../lib/path'

const props = defineProps<{
  open: boolean
  targetPaths: string[]
}>()

const emit = defineEmits<{
  (e: 'update:open', val: boolean): void
  (e: 'finished'): void
}>()

// 配置项
const sourceNetease = ref(true)
const sourceQq = ref(true)
const sourceKugou = ref(true)
const sourceLrclib = ref(true)
const sourceItunes = ref(true)

const overwriteTitle = ref(true)
const overwriteArtist = ref(true)
const overwriteAlbum = ref(true)
const overwriteCover = ref(true)
const overwriteLyrics = ref(true)

const onlyMissing = ref(false)
const exportLrc = ref(false)
const minSimilarity = ref(60) // 百分比
const concurrency = ref(4)

// 运行态
const isProcessing = ref(false)
const progressCurrent = ref(0)
const progressTotal = ref(0)
const logs = ref<BatchMatchProgress[]>([])
const summary = ref<BatchMatchSummary | null>(null)
let unlistenProgress: (() => void) | null = null

const progressPercent = computed(() => {
  if (progressTotal.value === 0) return 0
  return Math.min(100, Math.round((progressCurrent.value / progressTotal.value) * 100))
})

async function startMatching() {
  if (props.targetPaths.length === 0 || isProcessing.value) return

  isProcessing.value = true
  progressCurrent.value = 0
  progressTotal.value = props.targetPaths.length
  logs.value = []
  summary.value = null

  // 整理音源
  const sources: MusicSourceId[] = []
  if (sourceNetease.value) sources.push('netease')
  if (sourceQq.value) sources.push('qqmusic')
  if (sourceKugou.value) sources.push('kugou')
  if (sourceLrclib.value) sources.push('lrclib')
  if (sourceItunes.value) sources.push('itunes')

  const options: BatchMatchOptions = {
    sources: sources.length > 0 ? sources : undefined,
    overwrite_title: overwriteTitle.value,
    overwrite_artist: overwriteArtist.value,
    overwrite_album: overwriteAlbum.value,
    overwrite_cover: overwriteCover.value,
    overwrite_lyrics: overwriteLyrics.value,
    only_missing: onlyMissing.value,
    export_lrc: exportLrc.value,
    min_similarity: minSimilarity.value / 100,
    concurrency: concurrency.value,
  }

  // 监听后端推送进度
  try {
    unlistenProgress = await listenBatchMatchProgress((event) => {
      progressCurrent.value = event.current
      progressTotal.value = event.total
      logs.value.unshift(event) // 最新的排在上方
    })

    const res = await batchAutoMatch(props.targetPaths, options)
    summary.value = res
  } catch (err) {
    logs.value.unshift({
      current: progressCurrent.value,
      total: progressTotal.value,
      current_path: '',
      current_title: '批量匹配过程发生异常',
      status: 'failed',
      matched_source: null,
      matched_title: null,
      matched_artist: null,
      similarity: null,
      error: String(err),
    })
  } finally {
    isProcessing.value = false
    if (unlistenProgress) {
      unlistenProgress()
      unlistenProgress = null
    }
  }
}

function handleClose() {
  if (isProcessing.value) {
    if (!confirm('批量匹配仍在进行中，确定要关闭窗口吗？')) {
      return
    }
  }
  if (unlistenProgress) {
    unlistenProgress()
    unlistenProgress = null
  }
  emit('update:open', false)
  if (summary.value && summary.value.matched_count > 0) {
    emit('finished')
  }
}

onUnmounted(() => {
  if (unlistenProgress) {
    unlistenProgress()
    unlistenProgress = null
  }
})

function sourceLabel(src: MusicSourceId | null): string {
  switch (src) {
    case 'netease':
      return '网易云'
    case 'qqmusic':
      return 'QQ音乐'
    case 'kugou':
      return '酷狗'
    case 'lrclib':
      return 'LRCLIB'
    case 'itunes':
      return 'iTunes'
    default:
      return '在线'
  }
}
</script>

<template>
  <div v-if="open" class="batch-dialog-overlay" @click.self="handleClose">
    <div class="batch-dialog" role="dialog" aria-modal="true">
      <div class="dialog-header">
        <div class="header-title-row">
          <span class="sparkle-icon">✨</span>
          <h3 class="dialog-title">一键批量自动匹配与刮削</h3>
        </div>
        <button class="close-btn" type="button" @click="handleClose">✕</button>
      </div>

      <div class="dialog-content">
        <!-- 配置态 -->
        <div v-if="!isProcessing && !summary" class="config-view">
          <p class="section-intro">
            将对选中的 <strong class="highlight-count">{{ targetPaths.length }}</strong> 首音频执行智能多源并发检索，根据相似度自动提取并写入完整元数据、内嵌歌词与高清封面。
          </p>

          <div class="form-section">
            <div class="section-title">检索音源</div>
            <div class="checkbox-group">
              <label class="cb-label">
                <input v-model="sourceNetease" type="checkbox" /> 网易云音乐
              </label>
              <label class="cb-label">
                <input v-model="sourceQq" type="checkbox" /> QQ音乐
              </label>
              <label class="cb-label">
                <input v-model="sourceKugou" type="checkbox" /> 酷狗音乐
              </label>
              <label class="cb-label">
                <input v-model="sourceLrclib" type="checkbox" /> LRCLIB (歌词库)
              </label>
              <label class="cb-label">
                <input v-model="sourceItunes" type="checkbox" /> Apple / iTunes
              </label>
            </div>
          </div>

          <div class="form-section">
            <div class="section-title">匹配与覆盖内容</div>
            <div class="checkbox-group">
              <label class="cb-label">
                <input v-model="overwriteTitle" type="checkbox" /> 歌名
              </label>
              <label class="cb-label">
                <input v-model="overwriteArtist" type="checkbox" /> 艺术家
              </label>
              <label class="cb-label">
                <input v-model="overwriteAlbum" type="checkbox" /> 专辑名
              </label>
              <label class="cb-label">
                <input v-model="overwriteCover" type="checkbox" /> 内嵌封面
              </label>
              <label class="cb-label">
                <input v-model="overwriteLyrics" type="checkbox" /> 内嵌歌词
              </label>
            </div>
          </div>

          <div class="form-section">
            <div class="section-title">保护与高级设置</div>
            <div class="advanced-group">
              <label class="cb-label adv-cb">
                <input v-model="onlyMissing" type="checkbox" />
                <span><strong>仅补全缺失字段</strong>（不覆盖文件已有元数据/封面）</span>
              </label>
              <label class="cb-label adv-cb">
                <input v-model="exportLrc" type="checkbox" />
                <span>同步导出 <code>.lrc</code> 侧载歌词文件到同目录</span>
              </label>

              <div class="range-row">
                <span class="range-label">最小匹配相似度: <strong>{{ minSimilarity }}%</strong></span>
                <input v-model.number="minSimilarity" type="range" min="40" max="95" step="5" class="slider" />
                <span class="range-tip">低于 {{ minSimilarity }}% 自动跳过，防误匹配</span>
              </div>

              <div class="range-row">
                <span class="range-label">并发并发检索线程: <strong>{{ concurrency }}</strong></span>
                <input v-model.number="concurrency" type="range" min="1" max="8" step="1" class="slider" />
                <span class="range-tip">建议 4 线程，兼顾速度与稳定性</span>
              </div>
            </div>
          </div>
        </div>

        <!-- 处理与结果态 -->
        <div v-else class="progress-view">
          <div class="progress-header">
            <div class="progress-info">
              <span v-if="isProcessing" class="status-badge running">正在匹配刮削中…</span>
              <span v-else class="status-badge done">✓ 批量匹配刮削已完成</span>
              <span class="progress-counts">{{ progressCurrent }} / {{ progressTotal }} 首 ({{ progressPercent }}%)</span>
            </div>
            <div class="progress-bar-bg">
              <div class="progress-bar-fill" :style="{ width: `${progressPercent}%` }"></div>
            </div>
          </div>

          <!-- 汇总信息 -->
          <div v-if="summary" class="summary-cards">
            <div class="sum-card success">
              <span class="sum-num">{{ summary.matched_count }}</span>
              <span class="sum-txt">匹配成功</span>
            </div>
            <div class="sum-card skipped">
              <span class="sum-num">{{ summary.skipped_count }}</span>
              <span class="sum-txt">未匹配跳过</span>
            </div>
            <div class="sum-card failed">
              <span class="sum-num">{{ summary.failed_count }}</span>
              <span class="sum-txt">处理失败</span>
            </div>
          </div>

          <!-- 动态日志列表 -->
          <div class="log-container">
            <div v-for="(log, idx) in logs" :key="idx" class="log-item" :class="log.status">
              <div class="log-top">
                <span class="log-tag" :class="log.status">
                  {{ log.status === 'matched' ? '✓ 匹配' : log.status === 'skipped' ? '⊘ 跳过' : '✕ 失败' }}
                </span>
                <span class="log-file" :title="log.current_path">{{ fileName(log.current_path) }}</span>
                <span v-if="log.similarity" class="log-sim">{{ Math.round(log.similarity * 100) }}% 匹配</span>
                <span v-if="log.matched_source" class="log-source">[{{ sourceLabel(log.matched_source) }}]</span>
              </div>
              <div v-if="log.matched_title || log.error" class="log-detail">
                <span v-if="log.matched_title" class="matched-meta">
                  目标: {{ log.matched_artist ? log.matched_artist + ' - ' : '' }}{{ log.matched_title }}
                </span>
                <span v-if="log.error" class="error-msg">{{ log.error }}</span>
              </div>
            </div>
          </div>
        </div>
      </div>

      <div class="dialog-footer">
        <template v-if="!isProcessing && !summary">
          <button class="btn btn-ghost" type="button" @click="handleClose">取消</button>
          <button
            class="btn btn-primary"
            type="button"
            :disabled="targetPaths.length === 0"
            @click="startMatching"
          >
            开始自动刮削 ({{ targetPaths.length }} 首)
          </button>
        </template>
        <template v-else-if="isProcessing">
          <button class="btn btn-ghost" type="button" @click="handleClose">后台运行并关闭</button>
        </template>
        <template v-else>
          <button class="btn btn-primary" type="button" @click="handleClose">完成并刷新列表</button>
        </template>
      </div>
    </div>
  </div>
</template>

<style scoped>
.batch-dialog-overlay {
  position: fixed;
  inset: 0;
  background: rgba(0, 0, 0, 0.65);
  display: flex;
  align-items: center;
  justify-content: center;
  z-index: 1000;
  backdrop-filter: blur(4px);
}

.batch-dialog {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 12px;
  width: 90vw;
  max-width: 640px;
  max-height: 85vh;
  display: flex;
  flex-direction: column;
  box-shadow: 0 20px 48px rgba(0, 0, 0, 0.4);
  overflow: hidden;
}

.dialog-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 16px 20px;
  border-bottom: 1px solid var(--border);
  background: var(--panel-2);
}

.header-title-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.sparkle-icon {
  font-size: 18px;
}

.dialog-title {
  margin: 0;
  font-size: 16px;
  font-weight: 700;
  color: var(--text);
}

.close-btn {
  background: transparent;
  border: none;
  font-size: 16px;
  color: var(--text-dim);
  cursor: pointer;
  padding: 4px 8px;
  border-radius: 4px;
}

.close-btn:hover {
  background: var(--hover);
  color: var(--text);
}

.dialog-content {
  padding: 20px;
  overflow-y: auto;
  flex: 1 1 auto;
}

.section-intro {
  margin: 0 0 16px;
  font-size: 13px;
  color: var(--text-dim);
  line-height: 1.5;
}

.highlight-count {
  color: var(--accent);
  font-weight: 700;
}

.form-section {
  margin-bottom: 18px;
  padding-bottom: 16px;
  border-bottom: 1px solid var(--border);
}

.form-section:last-child {
  border-bottom: none;
  margin-bottom: 0;
  padding-bottom: 0;
}

.section-title {
  font-size: 13px;
  font-weight: 700;
  color: var(--text);
  margin-bottom: 10px;
}

.checkbox-group {
  display: flex;
  flex-wrap: wrap;
  gap: 12px 18px;
}

.cb-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  color: var(--text);
  cursor: pointer;
  user-select: none;
}

.advanced-group {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.adv-cb {
  font-size: 13px;
}

.adv-cb code {
  font-family: var(--mono);
  background: var(--hover);
  padding: 2px 4px;
  border-radius: 4px;
}

.range-row {
  display: flex;
  align-items: center;
  gap: 12px;
  margin-top: 4px;
}

.range-label {
  font-size: 12.5px;
  color: var(--text);
  min-width: 170px;
}

.slider {
  flex: 1;
  accent-color: var(--accent);
  cursor: pointer;
}

.range-tip {
  font-size: 11.5px;
  color: var(--text-dim);
  min-width: 130px;
  text-align: right;
}

/* 进度与日志态 */
.progress-view {
  display: flex;
  flex-direction: column;
  gap: 16px;
}

.progress-header {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.progress-info {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.status-badge {
  font-size: 12px;
  font-weight: 700;
  padding: 3px 8px;
  border-radius: 4px;
}

.status-badge.running {
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  color: var(--accent);
}

.status-badge.done {
  background: color-mix(in srgb, #22c55e 15%, transparent);
  color: #22c55e;
}

.progress-counts {
  font-size: 12.5px;
  font-family: var(--mono);
  color: var(--text-dim);
}

.progress-bar-bg {
  width: 100%;
  height: 8px;
  background: var(--hover);
  border-radius: 4px;
  overflow: hidden;
}

.progress-bar-fill {
  height: 100%;
  background: var(--accent);
  border-radius: 4px;
  transition: width 0.15s ease;
}

.summary-cards {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
}

.sum-card {
  padding: 10px;
  border-radius: 8px;
  display: flex;
  flex-direction: column;
  align-items: center;
  background: var(--panel-2);
  border: 1px solid var(--border);
}

.sum-card.success .sum-num {
  color: #22c55e;
}

.sum-card.skipped .sum-num {
  color: var(--text-dim);
}

.sum-card.failed .sum-num {
  color: var(--danger);
}

.sum-num {
  font-size: 20px;
  font-weight: 800;
  font-family: var(--mono);
}

.sum-txt {
  font-size: 11.5px;
  color: var(--text-dim);
}

.log-container {
  max-height: 280px;
  overflow-y: auto;
  border: 1px solid var(--border);
  border-radius: 8px;
  background: var(--bg);
  padding: 8px;
  display: flex;
  flex-direction: column;
  gap: 6px;
}

.log-item {
  padding: 6px 8px;
  border-radius: 6px;
  background: var(--panel-2);
  font-size: 12px;
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.log-top {
  display: flex;
  align-items: center;
  gap: 8px;
}

.log-tag {
  font-size: 11px;
  font-weight: 700;
  padding: 2px 6px;
  border-radius: 4px;
  flex-shrink: 0;
}

.log-tag.matched {
  background: color-mix(in srgb, #22c55e 15%, transparent);
  color: #22c55e;
}

.log-tag.skipped {
  background: var(--hover);
  color: var(--text-dim);
}

.log-tag.failed {
  background: color-mix(in srgb, var(--danger) 15%, transparent);
  color: var(--danger);
}

.log-file {
  font-family: var(--mono);
  color: var(--text);
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
  flex: 1;
}

.log-sim {
  font-family: var(--mono);
  font-size: 11px;
  color: var(--accent);
  flex-shrink: 0;
}

.log-source {
  font-size: 11px;
  color: var(--text-dim);
  flex-shrink: 0;
}

.log-detail {
  padding-left: 56px;
  font-size: 11.5px;
}

.matched-meta {
  color: var(--text-dim);
}

.error-msg {
  color: var(--danger);
}

.dialog-footer {
  display: flex;
  align-items: center;
  justify-content: flex-end;
  gap: 10px;
  padding: 14px 20px;
  border-top: 1px solid var(--border);
  background: var(--panel-2);
}

.btn {
  padding: 7px 16px;
  font-size: 13px;
  font-weight: 600;
  border-radius: 6px;
  cursor: pointer;
  transition: all 0.12s;
}

.btn:active {
  transform: translateY(1px);
}

.btn-ghost {
  background: transparent;
  color: var(--text);
  border: 1px solid var(--border);
}

.btn-ghost:hover {
  background: var(--hover);
}

.btn-primary {
  background: var(--accent);
  color: var(--accent-ink);
  border: none;
}

.btn-primary:hover {
  filter: brightness(1.08);
}

.btn-primary:disabled {
  opacity: 0.4;
  cursor: not-allowed;
}
</style>
