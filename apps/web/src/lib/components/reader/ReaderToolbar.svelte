<script lang="ts">
	import { ChevronLeft, Languages, Minus, Plus, Sparkles, Settings, PanelRight, Volume2 } from 'lucide-svelte';

	let { readerConfig = $bindable(), showSidebar = $bindable(), showTTS = $bindable(), immersiveMode = $bindable(), bookId, chapterIndex } = $props<{
		readerConfig: {
			fontSize: number;
			lineHeight: number;
			fontFamily: string;
			theme: string;
			maxWidth: number;
			highlightEntities: boolean;
		};
		showSidebar: boolean;
		showTTS: boolean;
		immersiveMode: boolean;
		bookId: string;
		chapterIndex: number;
	}>();

	let showSettings = $state(false);
</script>

<div class="flex h-12 shrink-0 items-center justify-between border-b border-ink-800/30 bg-ink-950/90 px-4 backdrop-blur-sm">
	<!-- Left: Navigation -->
	<div class="flex items-center gap-2">
		<a href="/library/{bookId}" aria-label="返回书籍详情" class="rounded-md p-1.5 text-ink-400 hover:bg-ink-800/50 hover:text-ink-100 transition-colors">
			<ChevronLeft size={16} strokeWidth={2} />
		</a>
		<span class="text-sm text-ink-300">第 {chapterIndex + 1} 章</span>
	</div>

	<!-- Center: Title (truncated) -->
	<div class="absolute left-1/2 -translate-x-1/2 text-sm font-medium text-ink-200 max-w-[40%] truncate">
		<!-- Book title would come from store -->
	</div>

	<!-- Right: Actions -->
	<div class="flex items-center gap-1">
		<!-- Font size controls -->
		<button
			type="button"
			onclick={() => readerConfig.fontSize = Math.max(12, readerConfig.fontSize - 1)}
			aria-label="缩小字体"
			class="rounded-md p-1.5 text-ink-400 hover:bg-ink-800/50 hover:text-ink-100 transition-colors"
			title="缩小字体"
		>
			<Minus size={16} strokeWidth={2} />
		</button>
		<span class="min-w-[2rem] text-center text-xs text-ink-400 tabular-nums">{readerConfig.fontSize}</span>
		<button
			type="button"
			onclick={() => readerConfig.fontSize = Math.min(32, readerConfig.fontSize + 1)}
			aria-label="放大字体"
			class="rounded-md p-1.5 text-ink-400 hover:bg-ink-800/50 hover:text-ink-100 transition-colors"
			title="放大字体"
		>
			<Plus size={16} strokeWidth={2} />
		</button>

		<!-- Divider -->
		<div class="mx-2 h-4 w-px bg-ink-700/50"></div>

		<!-- Entity highlight toggle -->
		<button
			type="button"
			onclick={() => readerConfig.highlightEntities = !readerConfig.highlightEntities}
			aria-label="切换实体高亮"
			aria-pressed={readerConfig.highlightEntities}
			class="rounded-md p-1.5 transition-colors {readerConfig.highlightEntities ? 'bg-accent-500/10 text-accent-400' : 'text-ink-400 hover:bg-ink-800/50'}"
			title="实体高亮"
		>
			<Sparkles size={16} strokeWidth={2} />
		</button>

		<!-- TTS toggle -->
		<button
			type="button"
			onclick={() => showTTS = !showTTS}
			aria-label="切换语音朗读"
			aria-pressed={showTTS}
			class="rounded-md p-1.5 transition-colors {showTTS ? 'bg-accent-500/10 text-accent-400' : 'text-ink-400 hover:bg-ink-800/50'}"
			title="语音朗读"
		>
			<Volume2 size={16} strokeWidth={2} />
		</button>

		<!-- Immersive translation toggle -->
		<button
			type="button"
			onclick={() => immersiveMode = !immersiveMode}
			aria-label="切换沉浸式翻译"
			aria-pressed={immersiveMode}
			class="rounded-md p-1.5 transition-colors {immersiveMode ? 'bg-accent-500/10 text-accent-400' : 'text-ink-400 hover:bg-ink-800/50'}"
			title="沉浸式翻译"
		>
			<Languages size={16} strokeWidth={2} />
		</button>

		<!-- Settings -->
		<button
			type="button"
			onclick={() => showSettings = !showSettings}
			aria-label="打开阅读设置"
			aria-pressed={showSettings}
			class="rounded-md p-1.5 text-ink-400 hover:bg-ink-800/50 hover:text-ink-100 transition-colors"
			title="阅读设置"
		>
			<Settings size={16} strokeWidth={2} />
		</button>

		<!-- Workbench toggle -->
		<button
			type="button"
			onclick={() => showSidebar = !showSidebar}
			aria-label="切换阅读工作台"
			aria-pressed={showSidebar}
			class="rounded-md p-1.5 transition-colors"
			class:text-accent-400={showSidebar}
			class:text-ink-400={!showSidebar}
			title="阅读工作台"
		>
			<PanelRight size={16} strokeWidth={2} />
		</button>
	</div>
</div>

<!-- Settings panel (dropdown) -->
{#if showSettings}
	<div class="absolute right-4 top-14 z-50 w-72 rounded-xl border border-ink-700/50 bg-ink-900 p-4 shadow-xl">
		<h4 class="mb-3 text-sm font-medium text-ink-100">阅读设置</h4>

		<!-- Live preview -->
		<div
			class="mb-3 rounded-lg border border-ink-800/50 bg-ink-950/50 px-3 py-2 overflow-hidden"
			style="font-size: {readerConfig.fontSize}px; line-height: {readerConfig.lineHeight}; font-family: var(--font-{readerConfig.fontFamily}, serif); max-height: 4.5em;"
		>
			<p class="text-ink-300 line-clamp-3">月光从窗帘的缝隙中透进来，在地板上画出一道细长的银白色光带。</p>
		</div>

		<!-- Font family -->
		<div class="mb-3">
			<label for="reader-font-family" class="text-xs text-ink-400">字体</label>
			<select id="reader-font-family" bind:value={readerConfig.fontFamily} class="mt-1 w-full rounded-lg border border-ink-700/50 bg-ink-800 px-3 py-1.5 text-sm text-ink-200">
				<option value="serif">衬线体</option>
				<option value="sans">无衬线</option>
				<option value="mono">等宽</option>
			</select>
		</div>

		<!-- Font size -->
		<div class="mb-3">
			<label for="reader-font-size" class="text-xs text-ink-400">字号: {readerConfig.fontSize}px</label>
			<input id="reader-font-size" type="range" bind:value={readerConfig.fontSize} min="12" max="32" step="1" class="mt-1 w-full accent-accent-500" />
		</div>

		<!-- Line height -->
		<div class="mb-3">
			<label for="reader-line-height" class="text-xs text-ink-400">行高: {readerConfig.lineHeight}</label>
			<input id="reader-line-height" type="range" bind:value={readerConfig.lineHeight} min="1.2" max="2.5" step="0.1" class="mt-1 w-full accent-accent-500" />
		</div>

		<!-- Max width -->
		<div>
			<label for="reader-max-width" class="text-xs text-ink-400">最大宽度: {readerConfig.maxWidth}px</label>
			<input id="reader-max-width" type="range" bind:value={readerConfig.maxWidth} min="480" max="1200" step="40" class="mt-1 w-full accent-accent-500" />
		</div>
	</div>
{/if}
