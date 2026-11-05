<script setup lang="ts">
// 左栏：搜索框 + 展平列表。
import { computed, nextTick, onMounted, onUnmounted, ref } from 'vue'

import { getLastDir, listSongs, openSong } from '../api/songs'
import type { MissingField } from '../api/types'
import { filteredSongs } from '../store/selectors'
import {
  clearSelection,
  closeMissingFilter,
  initLastDir,
  MISSING_FIELDS,
  openMissingFilter,
  refreshFolder,
  requestSwitch,
  resetFailedStartupFolder,
  selectAllSongs,
  startFolderWatching,
  stopFolderWatching,
  scanMissing,
  setMissingChecks,
  songStore,
} from '../store/song'
import SongRow from './SongRow.vue'
import BatchAutoMatchDialog from './BatchAutoMatchDialog.vue'

const showAutoMatchDialog = ref(false)
const autoMatchTargetPaths = ref<string[]>([])

function triggerAutoMatch(): void {
  if (songStore.selectedPaths.length > 1) {
    autoMatchTargetPaths.value = [...songStore.selectedPaths]
  } else {
    autoMatchTargetPaths.value = filteredSongs.value.map((s) => s.path)
  }
  showAutoMatchDialog.value = true
}

function onAutoMatchFinished(): void {
  void refreshFolder()
}

function onKeydown(e: KeyboardEvent) {
  if (menuOpen.value && e.key === 'Escape') {
    e.preventDefault()
    closeMenu(true)
  }
  if (menuOpen.value && e.key === 'Tab') closeMenu(false)

  // 全选快捷键 (Cmd+A / Ctrl+A)
  const target = e.target as HTMLElement | null
  const isTyping = target?.tagName === 'INPUT' || target?.tagName === 'TEXTAREA' || target?.isContentEditable
  if ((e.metaKey || e.ctrlKey) && e.key.toLowerCase() === 'a') {
    if (!isTyping) {
      e.preventDefault()
      selectAllSongs(filteredSongs.value.map((s) => s.path))
      return
    }
  }

  // 退出多选 (Escape)
  if (e.key === 'Escape' && songStore.selectedPaths.length > 1) {
    clearSelection()
    return
  }

  // 键盘上下按键切换歌曲 (ArrowUp / ArrowDown)
  if (!isTyping && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
    if (filteredSongs.value.length === 0) return
    e.preventDefault()

    const list = filteredSongs.value
    const currentPath = songStore.selectedPath
    let nextIdx = 0

    if (currentPath) {
      const idx = list.findIndex((s) => s.path === currentPath)
      if (idx !== -1) {
        if (e.key === 'ArrowDown') {
          nextIdx = Math.min(list.length - 1, idx + 1)
        } else {
          nextIdx = Math.max(0, idx - 1)
        }
      }
    }

    const nextSong = list[nextIdx]
    if (nextSong && nextSong.path !== currentPath) {
      void requestSwitch(nextSong.path, openSong)
      void nextTick(() => {
        const el = document.querySelector('.song-row.selected')
        el?.scrollIntoView({ block: 'nearest' })
      })
    }
  }
}

const missingLabels: Record<MissingField, string> = {
  title: '歌名',
  artist: '歌手',
  album: '专辑',
  cover: '封面',
  lyrics: '歌词',
}

function isMissingChecked(field: MissingField): boolean {
  return songStore.missingChecks.includes(field)
}

function toggleMissingField(field: MissingField, event: Event): void {
  const checked = (event.target as HTMLInputElement).checked
  const next = songStore.missingChecks.filter((item) => item !== field)
  if (checked) next.push(field)
  void setMissingChecks(next)
}

function retryMissingScan(): void {
  void scanMissing()
}

const missingEmptyState = computed(() => {
  if (
    !songStore.missingFilterEnabled ||
    songStore.missingScanState !== 'done' ||
    songStore.searchQuery.trim() !== ''
  ) {
    return null
  }
  if (Object.keys(songStore.missingByPath).length > 0) return null
  if (songStore.missingScanErrors.length > 0) {
    return {
      title: '扫描结果不完整',
      description: '部分歌曲读取失败，请查看上方提示后重试。',
    }
  }
  return {
    title: '没有缺失所选字段的歌曲',
    description: '当前文件夹没有缺少所选字段的歌曲',
  }
})

let mounted = false
const listArea = ref<HTMLElement | null>(null)
const menu = ref<HTMLElement | null>(null)
const refreshButton = ref<HTMLButtonElement | null>(null)
const menuOpen = ref(false)
const menuPosition = ref({ left: '0px', top: '0px' })
let menuOrigin: HTMLElement | null = null

function closeMenu(restoreFocus = false): void {
  menuOpen.value = false
  if (restoreFocus) menuOrigin?.focus()
}

function showMenu(event: MouseEvent | KeyboardEvent): void {
  event.preventDefault()
  menuOrigin = document.activeElement instanceof HTMLElement ? document.activeElement : listArea.value
  const rect = listArea.value?.getBoundingClientRect()
  const x = event instanceof MouseEvent ? event.clientX : (rect?.left ?? 0) + 12
  const y = event instanceof MouseEvent ? event.clientY : (rect?.top ?? 0) + 12
  menuPosition.value = {
    left: `${Math.max(0, Math.min(x, window.innerWidth - 140))}px`,
    top: `${Math.max(0, Math.min(y, window.innerHeight - 50))}px`,
  }
  menuOpen.value = true
  void nextTick(() => refreshButton.value?.focus())
}

function onListKeydown(event: KeyboardEvent): void {
  if (event.key === 'ContextMenu' || (event.shiftKey && event.key === 'F10')) showMenu(event)
}

function onOutsidePointer(event: PointerEvent): void {
  if (menuOpen.value && !menu.value?.contains(event.target as Node)) closeMenu()
}

function manualRefresh(): void {
  closeMenu(true)
  void refreshFolder()
}

onMounted(() => {
  mounted = true
  window.addEventListener('keydown', onKeydown)
  window.addEventListener('pointerdown', onOutsidePointer)
  const startupEpoch = songStore.folderEpoch
  void (async () => {
    // 事件订阅先就绪；目录恢复与手动打开共用 store 的 epoch/监听链路。
    await startFolderWatching()
    if (!mounted || songStore.folderEpoch !== startupEpoch || songStore.folderPath !== null) return
    const dir = await getLastDir()
    if (!mounted || songStore.folderEpoch !== startupEpoch || songStore.folderPath !== null || !dir) return
    const loading = initLastDir(dir, listSongs)
    const loadingEpoch = songStore.folderEpoch
    try {
      await loading
    } catch {
      if (mounted) resetFailedStartupFolder(loadingEpoch)
    }
  })().catch(() => {})
})
onUnmounted(() => {
  mounted = false
  window.removeEventListener('keydown', onKeydown)
  window.removeEventListener('pointerdown', onOutsidePointer)
  stopFolderWatching()
})

</script>

<template>
  <aside class="song-list">
    <div class="list-head">
      <input
        v-model="songStore.searchQuery"
        class="search-input"
        type="text"
        placeholder="搜索歌名 / 作者"
      />
      <div class="list-head-actions">
        <button
          class="batch-match-btn"
          type="button"
          :disabled="songStore.folderPath === null || filteredSongs.length === 0"
          title="批量自动匹配与刮削标签/封面/歌词"
          @click="triggerAutoMatch"
        >
          ✨ 批量刮削
        </button>
        <button
          class="missing-filter-btn"
          type="button"
          :disabled="songStore.folderPath === null"
          @click="openMissingFilter()"
        >
          筛选缺失
        </button>
      </div>
    </div>

    <!-- 批量选择状态栏 -->
    <div v-if="songStore.selectedPaths.length > 1" class="batch-status-bar">
      <span>已选 {{ songStore.selectedPaths.length }} 项</span>
      <div class="batch-status-actions">
        <button
          type="button"
          class="batch-status-btn primary"
          @click="triggerAutoMatch"
        >
          ✨ 刮削选定
        </button>
        <button
          type="button"
          class="batch-status-btn"
          @click="selectAllSongs(filteredSongs.map((s) => s.path))"
        >
          全选 ({{ filteredSongs.length }})
        </button>
        <button type="button" class="batch-status-btn" @click="clearSelection">
          取消
        </button>
      </div>
    </div>

    <div v-if="songStore.missingFilterEnabled" class="missing-panel" data-testid="missing-panel">
      <div class="missing-panel-head">
        <span>缺失字段</span>
        <span class="missing-head-actions">
          <!-- 刷新：常驻入口，复用 retryMissingScan → scanMissing（missing-filter-refresh #129）。
               禁挂 .missing-close-btn class（songlist.test.ts 唯一选择器要求）。 -->
          <button
            class="missing-refresh-btn"
            type="button"
            data-testid="missing-refresh-btn"
            @click="retryMissingScan"
          >
            刷新
          </button>
          <button class="missing-close-btn" type="button" @click="closeMissingFilter">关闭</button>
        </span>
      </div>
      <label v-for="field in MISSING_FIELDS" :key="field" class="missing-check">
        <input
          type="checkbox"
          :checked="isMissingChecked(field)"
          @change="toggleMissingField(field, $event)"
        />
        <span>{{ missingLabels[field] }}</span>
      </label>
      <p v-if="songStore.missingScanState === 'scanning'" class="missing-status">
        正在扫描缺失字段…
      </p>
      <p v-else-if="songStore.missingScanState === 'error'" class="missing-status missing-error">
        扫描失败：{{ songStore.missingScanError }}
        <button class="missing-retry-btn" type="button" @click="retryMissingScan">重试</button>
      </p>
      <p v-if="songStore.missingScanState === 'done' && songStore.missingScanErrors.length > 0" class="missing-status">
        {{ songStore.missingScanErrors.length }} 首歌曲读取失败，已保留其他结果
      </p>
    </div>

    <p v-if="songStore.folderWatchError || songStore.folderRefreshError" class="folder-error" role="status">
      <span v-if="songStore.folderWatchError">{{ songStore.folderWatchError }}</span>
      <span v-if="songStore.folderRefreshError">{{ songStore.folderRefreshError }}</span>
      <button type="button" :disabled="songStore.folderPath === null" @click="manualRefresh">重试刷新</button>
    </p>

    <div ref="listArea" class="list-area" tabindex="0" aria-label="歌曲列表" aria-haspopup="menu"
      @contextmenu="showMenu" @keydown="onListKeydown">
    <!-- 空态：未打开文件夹 -->
    <div v-if="songStore.folderPath === null" class="empty">
      <span class="empty-icon" aria-hidden="true">🗂️</span>
      <p class="empty-title">未打开文件夹</p>
      <p class="empty-desc">点击顶部「打开文件夹」选择本地音乐文件夹</p>
    </div>

    <!-- 空文件夹 / 无匹配 -->
    <div
      v-else-if="filteredSongs.length === 0"
      class="empty"
      data-testid="empty-state"
    >
      <span class="empty-icon" aria-hidden="true">🎵</span>
      <p class="empty-title">
        {{
          missingEmptyState?.title
            ? missingEmptyState.title
            : songStore.songs.length === 0
              ? '文件夹中没有音乐'
              : '无匹配结果'
        }}
      </p>
      <p class="empty-desc">
        {{
          missingEmptyState?.description
            ? missingEmptyState.description
            : songStore.songs.length === 0
              ? '当前文件夹没有 .flac / .mp3 文件'
              : '换个关键词试试'
        }}
      </p>
    </div>

    <!-- 列表 -->
    <ul v-else class="list">
      <SongRow
        v-for="song in filteredSongs"
        :key="song.path"
        :song="song"
        :missing="songStore.missingByPath[song.path]"
      />
    </ul>
    </div>
    <div v-if="menuOpen" ref="menu" class="list-menu" role="menu" aria-label="歌曲列表操作" :style="menuPosition">
      <button ref="refreshButton" role="menuitem" type="button" :disabled="songStore.folderPath === null"
        @click="manualRefresh" @keydown.down.prevent="refreshButton?.focus()" @keydown.up.prevent="refreshButton?.focus()">刷新</button>
    </div>

    <!-- 批量自动匹配模态框 -->
    <BatchAutoMatchDialog
      v-model:open="showAutoMatchDialog"
      :target-paths="autoMatchTargetPaths"
      @finished="onAutoMatchFinished"
    />
  </aside>
</template>

<style scoped>
.list-head-actions {
  display: flex;
  gap: 8px;
}

.batch-match-btn {
  flex: 1;
  padding: 6px 8px;
  border: 1px solid var(--accent);
  border-radius: 6px;
  background: color-mix(in srgb, var(--accent) 12%, transparent);
  color: var(--accent);
  font-size: 12px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.12s;
  white-space: nowrap;
}

.batch-match-btn:hover:not(:disabled) {
  background: var(--accent);
  color: var(--accent-ink);
}

.batch-match-btn:disabled {
  cursor: not-allowed;
  opacity: 0.5;
  border-color: var(--border);
  color: var(--text-dim);
  background: transparent;
}

.batch-status-btn.primary {
  border-color: var(--accent);
  color: var(--accent);
  font-weight: 700;
}

.batch-status-btn.primary:hover {
  background: var(--accent);
  color: var(--accent-ink);
}
.song-list {
  display: flex;
  flex-direction: column;
  width: 280px;
  min-width: 200px;
  background: var(--panel);
  border-right: 1px solid var(--border);
  flex: 0 0 auto;
}

.list-area {
  display: flex;
  flex-direction: column;
  flex: 1 1 auto;
  min-height: 0;
}
.list-area:focus-visible { outline: 1px solid var(--accent); outline-offset: -1px; }
.list-menu {
  position: fixed;
  z-index: 100;
  min-width: 130px;
  padding: 4px;
  background: var(--panel);
  border: 1px solid var(--border);
  border-radius: 6px;
}
.list-menu button {
  width: 100%;
  padding: 8px 12px;
  text-align: left;
  color: var(--text);
  background: transparent;
  border: 0;
  border-radius: 3px;
}
.list-menu button:hover:not(:disabled), .list-menu button:focus-visible { background: var(--hover); }
.list-menu button:disabled { opacity: 0.5; }
.folder-error {
  display: grid;
  gap: 4px;
  padding: 8px 10px;
  color: var(--danger, #e57373);
  font-size: 12px;
}
.folder-error button { justify-self: start; color: inherit; }

.list-head {
  display: flex;
  flex-direction: column;
  gap: 8px;
  padding: 10px;
  border-bottom: 1px solid var(--border);
}

.missing-filter-btn,
.missing-close-btn,
.missing-retry-btn,
.missing-refresh-btn {
  padding: 6px 8px;
  border: 1px solid var(--border);
  border-radius: 6px;
  background: transparent;
  color: var(--text-dim);
  font-size: 12px;
  cursor: pointer;
}

.missing-filter-btn:hover:not(:disabled),
.missing-close-btn:hover,
.missing-retry-btn:hover,
.missing-refresh-btn:hover {
  color: var(--text);
  background: var(--hover);
}

.missing-filter-btn:disabled {
  cursor: not-allowed;
  opacity: 0.5;
}

.missing-panel {
  display: grid;
  grid-template-columns: repeat(2, minmax(0, 1fr));
  gap: 7px 10px;
  padding: 9px 10px;
  border-bottom: 1px solid var(--border);
  background: var(--panel-2);
}

.missing-panel-head,
.missing-status {
  grid-column: 1 / -1;
}

.missing-panel-head {
  display: flex;
  align-items: center;
  justify-content: space-between;
  color: var(--text);
  font-size: 12px;
  font-weight: 600;
}

.missing-head-actions {
  display: flex;
  gap: 6px;
}

.missing-check {
  display: flex;
  align-items: center;
  gap: 6px;
  color: var(--text-dim);
  font-size: 12px;
}

.missing-check input {
  accent-color: var(--accent);
}

.missing-status {
  margin: 2px 0 0;
  color: var(--text-dim);
  font-size: 11px;
  line-height: 1.4;
}

.missing-error {
  color: var(--danger, #e57373);
}

.missing-retry-btn {
  margin-left: 5px;
  padding: 3px 6px;
  color: inherit;
}

.search-input {
  width: 100%;
  padding: 7px 10px;
  font-size: 12px;
  font-family: inherit;
  color: var(--text);
  background: var(--bg);
  border: 1px solid var(--border);
  border-radius: 6px;
}

.search-input::placeholder {
  color: var(--text-dim);
}

.search-input:focus {
  outline: none;
  border-color: var(--accent);
}

/* ---------- 空状态（design.md §6.1：图标 40px 35% 透明 + 标题 + 副说明统一） ---------- */
.empty {
  flex: 1 1 auto;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  gap: 8px;
  padding: 24px;
  color: var(--text-dim);
  text-align: center;
}

.empty-icon {
  font-size: 40px;
  line-height: 1;
  opacity: 0.35;
}

.empty-title {
  color: var(--text);
  font-weight: 600;
  font-size: 15px;
}

.empty-desc {
  font-size: 12px;
  max-width: 200px;
}

/* ---------- 列表 ---------- */
ul {
  list-style: none;
  overflow-y: auto;
  flex: 1 1 auto;
  min-height: 0;
  padding: 6px;
}

/* ---------- 批量选择状态栏 ---------- */
.batch-status-bar {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 6px 12px;
  background: var(--panel-2);
  border-bottom: 1px solid var(--border);
  font-size: 12px;
  color: var(--accent);
  font-weight: 600;
}

.batch-status-actions {
  display: flex;
  gap: 6px;
}

.batch-status-btn {
  background: none;
  border: 1px solid var(--border);
  border-radius: 4px;
  color: var(--text);
  font-size: 11px;
  padding: 2px 6px;
  cursor: pointer;
}

.batch-status-btn:hover {
  background: var(--hover);
  color: var(--accent);
}
</style>
