<script setup lang="ts">
import { computed, reactive, ref } from 'vue'
import { batchUpdateTags, pickCoverFile, readCoverPath } from '../api/songs'
import type { BatchTagFields, CoverInput } from '../api/types'
import { fileName, fileNameStem } from '../lib/path'
import { refreshFolder, songStore } from '../store/song'
import BatchAutoMatchDialog from './BatchAutoMatchDialog.vue'

const showAutoMatchDialog = ref(false)

// 封面状态
const batchCover = ref<CoverInput | null>(null)
const coverAction = ref<'keep' | 'set' | 'clear'>('keep')

// 歌名与文件名规则
const titleMode = ref<'keep' | 'sequence' | 'fixed'>('keep')
const sequenceBaseName = ref('')
const sequenceConnector = ref<'dash' | 'underscore' | 'bracket' | 'episode'>('dash')
const fixedTitle = ref('')
const renameFiles = ref(false)

// 音轨号
const autoTrackNumber = ref(false)
const setTrackTotal = ref(false)

// 常规元数据字段
const form = reactive({
  album: { enabled: false, value: '' },
  artist: { enabled: false, value: '' },
  album_artist: { enabled: false, value: '' },
  year: { enabled: false, value: '' },
  genre: { enabled: false, value: '' },
  lyrics: { enabled: false, action: 'clear' as 'clear' | 'custom', value: '' },
})

const isUpdating = ref(false)
const updateMessage = ref<{ type: 'success' | 'error'; text: string } | null>(null)

// 强制按自然数字排序（Natural Sort：确保 1, 2, ..., 9, 10, 11, 12，绝不出现 12 排在 2 前面）
const sortedPaths = computed(() => {
  return [...songStore.selectedPaths].sort((a, b) => {
    const nameA = fileName(a)
    const nameB = fileName(b)
    return nameA.localeCompare(nameB, 'zh-CN', { numeric: true, sensitivity: 'base' })
  })
})

const padLength = computed(() => {
  const len = sortedPaths.value.length
  return len >= 100 ? 3 : 2
})

function formatSeq(index: number): string {
  const numStr = String(index + 1).padStart(padLength.value, '0')
  switch (sequenceConnector.value) {
    case 'dash':
      return ` - ${numStr}`
    case 'underscore':
      return `_${numStr}`
    case 'bracket':
      return ` (${numStr})`
    case 'episode':
      return ` 第${numStr}期`
    default:
      return ` - ${numStr}`
  }
}

// 预览每一首计算出的新标题与新文件名
const previewItems = computed(() => {
  const list = sortedPaths.value
  return list.map((p, idx) => {
    const originalName = fileName(p)
    const stem = fileNameStem(p)
    const ext = originalName.includes('.') ? '.' + originalName.split('.').pop() : ''

    let newTitle: string | null = null
    if (titleMode.value === 'fixed') {
      newTitle = fixedTitle.value.trim()
    } else if (titleMode.value === 'sequence') {
      const base = sequenceBaseName.value.trim() || stem
      newTitle = base + formatSeq(idx)
    }

    let newFileName: string | null = null
    if (renameFiles.value && newTitle) {
      newFileName = `${newTitle}${ext}`
    }

    const trackStr = autoTrackNumber.value ? String(idx + 1) : null

    return {
      path: p,
      originalName,
      newTitle,
      newFileName,
      trackStr,
    }
  })
})

async function onChooseCover() {
  try {
    const cover = await pickCoverFile()
    if (cover) {
      batchCover.value = cover
      coverAction.value = 'set'
    }
  } catch (err) {
    console.error('选择封面失败', err)
  }
}

async function onDropCover(e: DragEvent) {
  const files = e.dataTransfer?.files
  if (!files || files.length === 0) return
  const path = (files[0] as unknown as { path?: string }).path
  if (path) {
    try {
      const cover = await readCoverPath(path)
      batchCover.value = cover
      coverAction.value = 'set'
    } catch (err) {
      alert(`无法读取该图片: ${err}`)
    }
  }
}

function removeCover() {
  batchCover.value = null
  coverAction.value = 'keep'
}

async function applyBatchUpdates() {
  if (sortedPaths.value.length === 0) return

  const fields: BatchTagFields = {}
  let hasAnyChange = false

  // 封面
  if (coverAction.value === 'set' && batchCover.value) {
    fields.cover = batchCover.value.data_url
    hasAnyChange = true
  } else if (coverAction.value === 'clear') {
    fields.cover = ''
    hasAnyChange = true
  }

  // 专辑、艺人、年份、流派
  if (form.album.enabled) {
    fields.album = form.album.value
    hasAnyChange = true
  }
  if (form.artist.enabled) {
    fields.artist = form.artist.value
    hasAnyChange = true
  }
  if (form.album_artist.enabled) {
    fields.album_artist = form.album_artist.value
    hasAnyChange = true
  }
  if (form.year.enabled) {
    fields.year = form.year.value
    hasAnyChange = true
  }
  if (form.genre.enabled) {
    fields.genre = form.genre.value
    hasAnyChange = true
  }

  // 歌词
  if (form.lyrics.enabled) {
    fields.lyrics = form.lyrics.action === 'clear' ? '' : form.lyrics.value
    hasAnyChange = true
  }

  // 总音轨号
  if (setTrackTotal.value) {
    fields.track_total = String(sortedPaths.value.length)
    hasAnyChange = true
  }

  // 音轨号序列
  if (autoTrackNumber.value) {
    const tracksMap: Record<string, string> = {}
    previewItems.value.forEach((item) => {
      if (item.trackStr) tracksMap[item.path] = item.trackStr
    })
    fields.tracks = tracksMap
    hasAnyChange = true
  }

  // 歌名规则
  if (titleMode.value !== 'keep') {
    const titlesMap: Record<string, string> = {}
    previewItems.value.forEach((item) => {
      if (item.newTitle) titlesMap[item.path] = item.newTitle
    })
    fields.titles = titlesMap
    hasAnyChange = true
  }

  // 重命名文件
  if (renameFiles.value && titleMode.value !== 'keep') {
    const renameMap: Record<string, string> = {}
    previewItems.value.forEach((item) => {
      if (item.newFileName) renameMap[item.path] = item.newFileName
    })
    fields.rename_files = renameMap
    hasAnyChange = true
  }

  if (!hasAnyChange) {
    updateMessage.value = { type: 'error', text: '请至少勾选或设置一项需要修改的内容' }
    return
  }

  isUpdating.value = true
  updateMessage.value = null

  try {
    const res = await batchUpdateTags(sortedPaths.value, fields)
    if (res.failure_count === 0) {
      updateMessage.value = {
        type: 'success',
        text: `✓ 成功批量更新全部 ${res.success_count} 首音频的信息！`,
      }
    } else {
      updateMessage.value = {
        type: res.success_count > 0 ? 'success' : 'error',
        text: `批量更新完成：成功 ${res.success_count} 首，失败 ${res.failure_count} 首。${res.errors.join('; ')}`,
      }
    }
    void refreshFolder()
  } catch (err) {
    updateMessage.value = {
      type: 'error',
      text: `批量操作失败: ${err instanceof Error ? err.message : String(err)}`,
    }
  } finally {
    isUpdating.value = false
  }
}
</script>

<template>
  <div class="batch-editor" data-testid="batch-editor">
    <div class="batch-header">
      <div class="batch-header-top">
        <div class="batch-badge">批量编辑模式</div>
        <button
          class="batch-auto-scrape-btn"
          type="button"
          @click="showAutoMatchDialog = true"
        >
          ✨ 一键批量自动刮削 (网易云/QQ/酷狗)
        </button>
      </div>
      <h2 class="batch-title">已选中 {{ songStore.selectedPaths.length }} 首音频</h2>
      <p class="batch-hint">
        勾选需要一键覆盖的内容。所有未勾选的项将完全保持原本文件的标签与内容不变。
      </p>
    </div>

    <!-- 1. 批量封面设置 -->
    <div class="section-card">
      <div class="section-card-title">
        <span class="sec-icon">🖼️</span> 封面图片批量覆盖
      </div>
      <div class="cover-setting-row">
        <div
          class="cover-drop-zone"
          :class="{ 'has-cover': !!batchCover }"
          @click="onChooseCover"
          @dragover.prevent
          @drop.prevent="onDropCover"
        >
          <img v-if="batchCover" :src="batchCover.data_url" class="cover-preview-img" alt="封面预览" />
          <div v-else class="drop-placeholder">
            <span class="placeholder-icon">📁</span>
            <span class="placeholder-text">点击选择图片 或 拖拽封面文件到此处</span>
          </div>
        </div>

        <div class="cover-controls">
          <label class="radio-option">
            <input v-model="coverAction" type="radio" value="keep" />
            <span>保持各自原有封面不变</span>
          </label>
          <label class="radio-option" :class="{ disabled: !batchCover }">
            <input v-model="coverAction" type="radio" value="set" :disabled="!batchCover" />
            <span><strong>一张图覆盖全部</strong>（将左侧选中的图片批量写入所有音频）</span>
          </label>
          <label class="radio-option">
            <input v-model="coverAction" type="radio" value="clear" />
            <span>清空全部选中音频的现有封面</span>
          </label>
          <button v-if="batchCover" class="btn-text-danger" type="button" @click="removeCover">
            ✕ 移除此图
          </button>
        </div>
      </div>
    </div>

    <!-- 2. 歌名与文件名规则批量处理（智能自然排序与顺序加后缀） -->
    <div class="section-card">
      <div class="section-card-title">
        <span class="sec-icon">🏷️</span> 歌名与文件名批量重命名
      </div>

      <div class="naming-options">
        <div class="mode-selector">
          <label class="radio-btn-label">
            <input v-model="titleMode" type="radio" value="keep" />
            <span>保持原歌名</span>
          </label>
          <label class="radio-btn-label">
            <input v-model="titleMode" type="radio" value="sequence" />
            <span><strong>按顺序自动加后缀编号</strong>（适合英语播客/章节）</span>
          </label>
          <label class="radio-btn-label">
            <input v-model="titleMode" type="radio" value="fixed" />
            <span>统一设为固定歌名</span>
          </label>
        </div>

        <!-- 顺序编号参数配置 -->
        <div v-if="titleMode === 'sequence'" class="sequence-config-box">
          <div class="form-row">
            <label class="row-label">新名称前缀/主体:</label>
            <input
              v-model="sequenceBaseName"
              type="text"
              class="input-box"
              placeholder="留空则保留原歌名，或填入如：BBC英语播客"
            />
          </div>

          <div class="form-row">
            <label class="row-label">自动后缀样式:</label>
            <div class="connector-radios">
              <label class="radio-option">
                <input v-model="sequenceConnector" type="radio" value="dash" />
                <span><code> - 01</code></span>
              </label>
              <label class="radio-option">
                <input v-model="sequenceConnector" type="radio" value="underscore" />
                <span><code>_01</code></span>
              </label>
              <label class="radio-option">
                <input v-model="sequenceConnector" type="radio" value="bracket" />
                <span><code> (01)</code></span>
              </label>
              <label class="radio-option">
                <input v-model="sequenceConnector" type="radio" value="episode" />
                <span><code> 第01期</code></span>
              </label>
            </div>
          </div>

          <div class="extra-switches">
            <label class="checkbox-option">
              <input v-model="renameFiles" type="checkbox" />
              <span><strong>同步重命名磁盘文件名</strong>（同目录 <code>.lrc</code> 歌词自动联动改名）</span>
            </label>
            <label class="checkbox-option">
              <input v-model="autoTrackNumber" type="checkbox" />
              <span>同步赋予音轨号 (Track 1..{{ sortedPaths.length }})</span>
            </label>
            <label class="checkbox-option">
              <input v-model="setTrackTotal" type="checkbox" />
              <span>同步设置总音轨号 (Total = {{ sortedPaths.length }})</span>
            </label>
          </div>
        </div>

        <!-- 固定名称配置 -->
        <div v-if="titleMode === 'fixed'" class="sequence-config-box">
          <div class="form-row">
            <label class="row-label">统一歌名:</label>
            <input v-model="fixedTitle" type="text" class="input-box" placeholder="填入统一名称" />
          </div>
          <label class="checkbox-option" style="margin-top: 8px;">
            <input v-model="renameFiles" type="checkbox" />
            <span>同步重命名磁盘文件名</span>
          </label>
        </div>
      </div>
    </div>

    <!-- 3. 常规标签一键覆盖（专辑、艺术家、流派、年份、歌词） -->
    <div class="section-card">
      <div class="section-card-title">
        <span class="sec-icon">📝</span> 专辑与基础标签一键覆盖
      </div>

      <div class="batch-fields">
        <!-- 专辑名 -->
        <div class="batch-field-row" :class="{ disabled: !form.album.enabled }">
          <label class="field-check">
            <input v-model="form.album.enabled" type="checkbox" />
            <span class="field-label">专辑名称 (Album)</span>
          </label>
          <input
            v-model="form.album.value"
            type="text"
            class="field-input"
            :disabled="!form.album.enabled"
            placeholder="例如：BBC 6 Minute English 2024"
          />
        </div>

        <!-- 艺术家 -->
        <div class="batch-field-row" :class="{ disabled: !form.artist.enabled }">
          <label class="field-check">
            <input v-model="form.artist.enabled" type="checkbox" />
            <span class="field-label">歌手 / 艺术家 (Artist)</span>
          </label>
          <input
            v-model="form.artist.value"
            type="text"
            class="field-input"
            :disabled="!form.artist.enabled"
            placeholder="例如：BBC Learning English"
          />
        </div>

        <!-- 专辑艺术家 -->
        <div class="batch-field-row" :class="{ disabled: !form.album_artist.enabled }">
          <label class="field-check">
            <input v-model="form.album_artist.enabled" type="checkbox" />
            <span class="field-label">专辑艺术家 (Album Artist)</span>
          </label>
          <input
            v-model="form.album_artist.value"
            type="text"
            class="field-input"
            :disabled="!form.album_artist.enabled"
            placeholder="例如：BBC"
          />
        </div>

        <!-- 年份 -->
        <div class="batch-field-row" :class="{ disabled: !form.year.enabled }">
          <label class="field-check">
            <input v-model="form.year.enabled" type="checkbox" />
            <span class="field-label">发行年份 (Year)</span>
          </label>
          <input
            v-model="form.year.value"
            type="text"
            class="field-input"
            :disabled="!form.year.enabled"
            placeholder="例如：2024"
          />
        </div>

        <!-- 流派 -->
        <div class="batch-field-row" :class="{ disabled: !form.genre.enabled }">
          <label class="field-check">
            <input v-model="form.genre.enabled" type="checkbox" />
            <span class="field-label">流派风格 (Genre)</span>
          </label>
          <input
            v-model="form.genre.value"
            type="text"
            class="field-input"
            :disabled="!form.genre.enabled"
            placeholder="例如：Podcast / English"
          />
        </div>

        <!-- 歌词 -->
        <div class="batch-field-row" :class="{ disabled: !form.lyrics.enabled }">
          <label class="field-check">
            <input v-model="form.lyrics.enabled" type="checkbox" />
            <span class="field-label">歌词内容 (Lyrics)</span>
          </label>
          <div class="lyrics-inline-box">
            <label class="radio-option">
              <input v-model="form.lyrics.action" type="radio" value="clear" :disabled="!form.lyrics.enabled" />
              <span>清空选中音频的所有内嵌歌词</span>
            </label>
            <label class="radio-option">
              <input v-model="form.lyrics.action" type="radio" value="custom" :disabled="!form.lyrics.enabled" />
              <span>统一填入文本:</span>
            </label>
            <input
              v-if="form.lyrics.action === 'custom'"
              v-model="form.lyrics.value"
              type="text"
              class="field-input"
              :disabled="!form.lyrics.enabled"
              placeholder="输入统一歌词说明"
            />
          </div>
        </div>
      </div>
    </div>

    <!-- 4. 实时效果对比预览表 -->
    <div v-if="titleMode !== 'keep' || renameFiles" class="section-card preview-card">
      <div class="section-card-title">
        <span class="sec-icon">👀</span> 变更实时效果预览（按自然数字排序）
      </div>
      <div class="preview-table-wrapper">
        <table class="preview-table">
          <thead>
            <tr>
              <th style="width: 45px;">序号</th>
              <th>原文件名</th>
              <th v-if="titleMode !== 'keep'">新歌名标签</th>
              <th v-if="renameFiles">新磁盘文件名</th>
            </tr>
          </thead>
          <tbody>
            <tr v-for="(item, idx) in previewItems.slice(0, 15)" :key="item.path">
              <td class="col-num">{{ idx + 1 }}</td>
              <td class="col-orig" :title="item.originalName">{{ item.originalName }}</td>
              <td v-if="titleMode !== 'keep'" class="col-new-title">
                {{ item.newTitle }}
              </td>
              <td v-if="renameFiles" class="col-new-file">
                {{ item.newFileName }}
              </td>
            </tr>
          </tbody>
        </table>
        <div v-if="previewItems.length > 15" class="preview-more-hint">
          … 以及其余 {{ previewItems.length - 15 }} 首文件
        </div>
      </div>
    </div>

    <!-- 底部操作与提示栏 -->
    <div class="batch-actions">
      <div v-if="updateMessage" class="message-banner" :class="updateMessage.type">
        {{ updateMessage.text }}
      </div>

      <button
        class="batch-apply-btn"
        type="button"
        :disabled="isUpdating"
        @click="applyBatchUpdates"
      >
        <span v-if="isUpdating">正在批量应用更新…</span>
        <span v-else>一键应用到选中的 {{ songStore.selectedPaths.length }} 首音频</span>
      </button>
    </div>

    <!-- 批量自动刮削模态框 -->
    <BatchAutoMatchDialog
      v-model:open="showAutoMatchDialog"
      :target-paths="songStore.selectedPaths"
      @finished="refreshFolder"
    />
  </div>
</template>

<style scoped>
.batch-editor {
  flex: 1 1 auto;
  min-height: 0;
  display: flex;
  flex-direction: column;
  padding: 24px 32px;
  background: var(--bg);
  overflow-y: auto;
  gap: 20px;
}

.batch-header {
  margin-bottom: 4px;
}

.batch-header-top {
  display: flex;
  align-items: center;
  justify-content: space-between;
  margin-bottom: 8px;
}

.batch-badge {
  display: inline-block;
  padding: 3px 8px;
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  color: var(--accent);
  font-size: 11px;
  font-weight: 700;
  border-radius: 4px;
}

.batch-auto-scrape-btn {
  background: color-mix(in srgb, var(--accent) 15%, transparent);
  color: var(--accent);
  border: 1px solid var(--accent);
  border-radius: 6px;
  padding: 6px 14px;
  font-size: 12.5px;
  font-weight: 700;
  cursor: pointer;
  transition: all 0.15s;
}

.batch-auto-scrape-btn:hover {
  background: var(--accent);
  color: var(--accent-ink);
}

.batch-title {
  margin: 0 0 6px 0;
  font-size: 20px;
  font-weight: 800;
  color: var(--text);
}

.batch-hint {
  margin: 0;
  font-size: 13px;
  color: var(--text-dim);
  line-height: 1.5;
}

/* 模块卡片 */
.section-card {
  background: var(--panel-2);
  border: 1px solid var(--border);
  border-radius: 10px;
  padding: 16px 20px;
  display: flex;
  flex-direction: column;
  gap: 14px;
}

.section-card-title {
  font-size: 14px;
  font-weight: 700;
  color: var(--text);
  display: flex;
  align-items: center;
  gap: 8px;
}

.sec-icon {
  font-size: 16px;
}

/* 封面设置 */
.cover-setting-row {
  display: flex;
  align-items: center;
  gap: 20px;
}

.cover-drop-zone {
  width: 110px;
  height: 110px;
  border: 2px dashed var(--border);
  border-radius: 8px;
  display: flex;
  align-items: center;
  justify-content: center;
  cursor: pointer;
  overflow: hidden;
  background: var(--panel);
  transition: all 0.15s;
  flex-shrink: 0;
}

.cover-drop-zone:hover {
  border-color: var(--accent);
  background: var(--hover);
}

.cover-drop-zone.has-cover {
  border-style: solid;
  border-color: var(--accent);
}

.cover-preview-img {
  width: 100%;
  height: 100%;
  object-fit: cover;
}

.drop-placeholder {
  display: flex;
  flex-direction: column;
  align-items: center;
  text-align: center;
  padding: 8px;
  color: var(--text-dim);
  gap: 4px;
}

.placeholder-icon {
  font-size: 24px;
}

.placeholder-text {
  font-size: 10.5px;
  line-height: 1.3;
}

.cover-controls {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.radio-option {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 13px;
  color: var(--text);
  cursor: pointer;
  user-select: none;
}

.radio-option.disabled {
  opacity: 0.5;
  cursor: not-allowed;
}

.btn-text-danger {
  background: none;
  border: none;
  color: var(--danger);
  font-size: 12px;
  cursor: pointer;
  align-self: flex-start;
  padding: 2px 0;
}

.btn-text-danger:hover {
  text-decoration: underline;
}

/* 命名规则选项 */
.naming-options {
  display: flex;
  flex-direction: column;
  gap: 12px;
}

.mode-selector {
  display: flex;
  gap: 16px;
  flex-wrap: wrap;
}

.radio-btn-label {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 13px;
  cursor: pointer;
}

.sequence-config-box {
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 8px;
  padding: 14px 16px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.form-row {
  display: flex;
  align-items: center;
  gap: 12px;
}

.row-label {
  font-size: 12.5px;
  color: var(--text-dim);
  min-width: 110px;
}

.input-box {
  flex: 1;
  padding: 7px 12px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--panel-2);
  color: var(--text);
  font-size: 13px;
}

.input-box:focus {
  outline: none;
  border-color: var(--accent);
}

.connector-radios {
  display: flex;
  gap: 14px;
  flex-wrap: wrap;
}

.connector-radios code {
  font-family: var(--mono);
  background: var(--hover);
  padding: 2px 4px;
  border-radius: 4px;
}

.extra-switches {
  display: flex;
  gap: 18px;
  flex-wrap: wrap;
  padding-top: 6px;
  border-top: 1px dashed var(--border);
}

.checkbox-option {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 12.5px;
  color: var(--text);
  cursor: pointer;
}

/* 字段行 */
.batch-fields {
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.batch-field-row {
  display: flex;
  align-items: center;
  gap: 16px;
}

.batch-field-row.disabled {
  opacity: 0.55;
}

.field-check {
  display: flex;
  align-items: center;
  gap: 8px;
  min-width: 180px;
  cursor: pointer;
  user-select: none;
}

.field-label {
  font-size: 13px;
  font-weight: 600;
  color: var(--text);
}

.field-input {
  flex: 1 1 auto;
  padding: 8px 12px;
  font-size: 13px;
  color: var(--text);
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 6px;
  outline: none;
  transition: border-color 0.12s;
}

.field-input:focus:not(:disabled) {
  border-color: var(--accent);
}

.field-input:disabled {
  background: var(--hover);
  cursor: not-allowed;
}

.lyrics-inline-box {
  flex: 1 1 auto;
  display: flex;
  align-items: center;
  gap: 14px;
}

/* 预览表格 */
.preview-table-wrapper {
  overflow-x: auto;
  max-height: 240px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: var(--panel);
}

.preview-table {
  width: 100%;
  border-collapse: collapse;
  font-size: 12px;
  text-align: left;
}

.preview-table th {
  background: var(--panel-2);
  padding: 6px 10px;
  font-weight: 700;
  color: var(--text-dim);
  border-bottom: 1px solid var(--border);
}

.preview-table td {
  padding: 6px 10px;
  border-bottom: 1px solid var(--border);
  font-family: var(--mono);
}

.col-num {
  color: var(--text-dim);
  text-align: center;
}

.col-orig {
  color: var(--text-dim);
  max-width: 180px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}

.col-new-title {
  color: var(--accent);
}

.col-new-file {
  color: #22c55e;
}

.preview-more-hint {
  padding: 6px;
  text-align: center;
  font-size: 11px;
  color: var(--text-dim);
}

/* 动作与状态栏 */
.batch-actions {
  display: flex;
  flex-direction: column;
  gap: 12px;
  margin-top: 4px;
}

.message-banner {
  padding: 10px 16px;
  border-radius: 6px;
  font-size: 13px;
  font-weight: 600;
}

.message-banner.success {
  background: color-mix(in srgb, #22c55e 15%, transparent);
  color: #22c55e;
  border: 1px solid color-mix(in srgb, #22c55e 30%, transparent);
}

.message-banner.error {
  background: color-mix(in srgb, var(--danger) 15%, transparent);
  color: var(--danger);
  border: 1px solid color-mix(in srgb, var(--danger) 30%, transparent);
}

.batch-apply-btn {
  padding: 12px 24px;
  font-size: 14px;
  font-weight: 700;
  border-radius: 8px;
  background: var(--accent);
  color: var(--accent-ink);
  border: none;
  cursor: pointer;
  transition: all 0.12s;
}

.batch-apply-btn:hover:not(:disabled) {
  filter: brightness(1.08);
}

.batch-apply-btn:disabled {
  opacity: 0.5;
  cursor: not-allowed;
}
</style>
