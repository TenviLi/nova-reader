<script lang="ts">
	import EntityHighlight from '$components/reader/EntityHighlight.svelte';
	import SelectionContextMenu from '$components/reader/SelectionContextMenu.svelte';
	import ImmersiveTranslation from '$components/reader/ImmersiveTranslation.svelte';
	import DictionaryTooltip from '$components/reader/DictionaryTooltip.svelte';
	import { readerStore } from '$stores/reader.svelte';
	import { api } from '$services/api';
	import { goto } from '$app/navigation';
	import { toast } from 'svelte-sonner';
	import { fade } from 'svelte/transition';

	let { bookId, chapterIndex, config, immersiveMode = $bindable(false), onprevchapter, onnextchapter } = $props<{
		bookId: string;
		chapterIndex: number;
		immersiveMode?: boolean;
		config: {
			fontSize: number;
			lineHeight: number;
			fontFamily: string;
			theme: string;
			maxWidth: number;
			highlightEntities: boolean;
			textIndent?: boolean;
			justify?: boolean;
		};
		onprevchapter?: () => void;
		onnextchapter?: () => void;
	}>();

	// Use store data directly — no duplicate fetching
	let content = $derived(readerStore.content);
	let entities = $derived(readerStore.entities);
	let annotations = $derived(readerStore.annotations.filter(a => a.chapter_index === chapterIndex));
	let loading = $derived(readerStore.loading);
	let loadError = $state('');

	// Text selection for context menu
	let selectedText = $state('');
	let contextMenuVisible = $state(false);
	let contextMenuX = $state(0);
	let contextMenuY = $state(0);
	let selectionStartOffset = $state(0);
	let selectionEndOffset = $state(0);

	function handleTextSelect(event: MouseEvent) {
		const selection = window.getSelection();
		if (!selection || selection.isCollapsed) {
			return;
		}

		const text = selection.toString().trim();
		if (text.length < 2) return;

		selectedText = text;

		// Calculate character offsets within content string
		const startIdx = content.indexOf(text);
		selectionStartOffset = startIdx >= 0 ? startIdx : 0;
		selectionEndOffset = selectionStartOffset + text.length;

		// Prevent menu from going off-screen (260px wide, ~320px max height)
		const menuWidth = 260;
		const menuHeight = 320;
		contextMenuX = Math.min(event.clientX, window.innerWidth - menuWidth - 8);
		contextMenuY = event.clientY + menuHeight > window.innerHeight
			? Math.max(8, event.clientY - menuHeight)
			: event.clientY;
		contextMenuVisible = true;
	}

	async function handleAnnotate(note: string, color: string) {
		try {
			await api.createAnnotation(bookId, {
				book_id: bookId,
				chapter_index: chapterIndex,
				selected_text: selectedText,
				note,
				color,
				start_offset: selectionStartOffset,
				end_offset: selectionEndOffset,
			});
			toast.success('批注已保存');
		} catch {
			toast.error('保存失败');
		}
	}

	// Render content with entity highlights
	function escapeHtml(str: string): string {
		return str
			.replace(/&/g, '&amp;')
			.replace(/</g, '&lt;')
			.replace(/>/g, '&gt;')
			.replace(/"/g, '&quot;')
			.replace(/'/g, '&#039;');
	}

	/** Convert plain text paragraphs (split by \n) into HTML <p> blocks with IDs */
	function textToHtml(text: string): string {
		const paragraphs = text.split(/\n+/).filter(p => p.trim());
		let charOffset = 0;
		return paragraphs.map((p, i) => {
			const idx = text.indexOf(p.trim(), charOffset);
			const offset = idx >= 0 ? idx : charOffset;
			charOffset = offset + p.trim().length;
			return `<p id="p-${i}" data-char-offset="${offset}" class="mb-4 leading-relaxed">${escapeHtml(p.trim())}</p>`;
		}).join('');
	}

	function renderContent(text: string, entityList: typeof entities): string {
		if (!config.highlightEntities || entityList.length === 0) return textToHtml(text);

		// Sort entities by start position (forward for linear scan)
		const sorted = [...entityList].sort((a, b) => a.start - b.start);
		const parts: string[] = [];
		let cursor = 0;

		for (const entity of sorted) {
			// Skip overlapping entities
			if (entity.start < cursor) continue;

			// Add text before entity
			if (entity.start > cursor) {
				parts.push(escapeHtml(text.slice(cursor, entity.start)));
			}

			// Add highlighted entity
			const match = text.slice(entity.start, entity.end);
			const typeClass = `entity-${escapeHtml(entity.type)}`;
			const safeName = escapeHtml(entity.name);
			const safeId = escapeHtml(entity.id);
			parts.push(`<span class="entity-highlight ${typeClass}" role="link" tabindex="0" data-entity-id="${safeId}" data-entity-name="${safeName}" title="${safeName}" aria-label="查看实体 ${safeName}">${escapeHtml(match)}</span>`);
			cursor = entity.end;
		}

		// Add remaining text
		if (cursor < text.length) {
			parts.push(escapeHtml(text.slice(cursor)));
		}

		// Convert newlines to paragraph breaks in the assembled HTML
		const raw = parts.join('');
		const paras = raw.split(/\n+/).filter(p => p.trim());
		return paras
			.map((p, i) => `<p id="p-${i}" class="mb-4 leading-relaxed">${p.trim()}</p>`)
			.join('');
	}

	let renderedContent = $derived(renderContent(content, entities));

	// ─── Touch Gestures for Mobile (swipe left/right for chapter nav) ────────
	let touchStartX = $state(0);
	let touchStartY = $state(0);
	let touchStartTime = $state(0);
	let swiping = $state(false);
	let lastSwipeTime = 0;

	function handleTouchStart(e: TouchEvent) {
		touchStartX = e.touches[0].clientX;
		touchStartY = e.touches[0].clientY;
		touchStartTime = Date.now();
		swiping = true;
	}

	function handleTouchEnd(e: TouchEvent) {
		if (!swiping) return;
		swiping = false;

		const endX = e.changedTouches[0].clientX;
		const endY = e.changedTouches[0].clientY;
		const diffX = endX - touchStartX;
		const diffY = endY - touchStartY;
		const elapsed = Date.now() - touchStartTime;

		// Debounce: prevent rapid consecutive swipes
		if (Date.now() - lastSwipeTime < 500) return;

		// Require: horizontal distance > 120px, mostly horizontal, velocity > 200px/s, within 800ms
		const velocity = Math.abs(diffX) / (elapsed / 1000);
		if (
			Math.abs(diffX) > 120 &&
			Math.abs(diffX) > Math.abs(diffY) * 2 &&
			velocity > 200 &&
			elapsed < 800
		) {
			lastSwipeTime = Date.now();
			if (diffX > 0) {
				onprevchapter?.();
			} else {
				onnextchapter?.();
			}
		}
	}

	// ─── Inline Dictionary Tooltip ───────────────────────────────────────────
	let dictWord = $state('');
	let dictVisible = $state(false);
	let dictPosition = $state({ x: 0, y: 0 });

	function handleDoubleClick(e: MouseEvent) {
		const selection = window.getSelection();
		const word = selection?.toString().trim();
		if (word && word.length >= 1 && word.length <= 20) {
			dictWord = word;
			dictPosition = { x: e.clientX, y: e.clientY };
			dictVisible = true;
		}
	}

	function findEntityElement(target: EventTarget | null): HTMLElement | null {
		if (!(target instanceof HTMLElement)) return null;
		return target.closest<HTMLElement>('[data-entity-id]');
	}

	function openEntity(entityId: string | undefined) {
		if (!entityId) return;
		goto(`/characters/${entityId}`);
	}

	function handleEntityClick(e: MouseEvent) {
		const entityEl = findEntityElement(e.target);
		if (!entityEl) return;
		openEntity(entityEl.dataset.entityId);
	}

	function handleEntityKeydown(e: KeyboardEvent) {
		if (e.key !== 'Enter' && e.key !== ' ') return;
		const entityEl = findEntityElement(e.target);
		if (!entityEl) return;
		e.preventDefault();
		openEntity(entityEl.dataset.entityId);
	}

	function entityNavigation(node: HTMLElement) {
		node.addEventListener('click', handleEntityClick);
		node.addEventListener('keydown', handleEntityKeydown);
		return {
			destroy() {
				node.removeEventListener('click', handleEntityClick);
				node.removeEventListener('keydown', handleEntityKeydown);
			},
		};
	}

	function readerInteractions(node: HTMLElement) {
		node.addEventListener('mouseup', handleTextSelect);
		node.addEventListener('dblclick', handleDoubleClick);
		node.addEventListener('touchstart', handleTouchStart);
		node.addEventListener('touchend', handleTouchEnd);
		return {
			destroy() {
				node.removeEventListener('mouseup', handleTextSelect);
				node.removeEventListener('dblclick', handleDoubleClick);
				node.removeEventListener('touchstart', handleTouchStart);
				node.removeEventListener('touchend', handleTouchEnd);
			},
		};
	}
</script>

<div
	role="document"
	aria-label="章节正文"
	class="reader-container mx-auto px-4 sm:px-8 py-8 sm:py-12"
	style="max-width: {config.maxWidth}px; font-size: {config.fontSize}px; line-height: {config.lineHeight}; font-family: var(--font-{config.fontFamily});{config.textIndent ? ' text-indent: 2em;' : ''}{config.justify ? ' text-align: justify;' : ''}"
	use:readerInteractions
	use:entityNavigation
>
	{#if loading}
		<div class="space-y-4 animate-pulse">
			{#each Array(20) as _}
				<div class="h-5 rounded bg-ink-800/30" style="width: {60 + Math.random() * 40}%"></div>
			{/each}
		</div>
	{:else if loadError}
		<div class="flex flex-col items-center justify-center py-16 gap-4 text-center">
			<div class="text-ink-500 text-sm">{loadError}</div>
			<button
				type="button"
				onclick={() => readerStore.loadChapter(chapterIndex)}
				class="px-4 py-2 rounded-lg bg-accent-500/10 text-accent-400 text-sm font-medium hover:bg-accent-500/20 transition-colors"
			>
				重新加载
			</button>
		</div>
	{:else if !content}
		<!-- No content available for this chapter -->
		<div class="flex flex-col items-center justify-center py-20 gap-4 text-center">
			<div class="rounded-full bg-ink-800/40 p-4">
				<svg class="w-8 h-8 text-ink-500" fill="none" stroke="currentColor" viewBox="0 0 24 24"><path stroke-linecap="round" stroke-linejoin="round" stroke-width="1.5" d="M12 6.253v13m0-13C10.832 5.477 9.246 5 7.5 5S4.168 5.477 3 6.253v13C4.168 18.477 5.754 18 7.5 18s3.332.477 4.5 1.253m0-13C13.168 5.477 14.754 5 16.5 5c1.747 0 3.332.477 4.5 1.253v13C19.832 18.477 18.247 18 16.5 18c-1.746 0-3.332.477-4.5 1.253"/></svg>
			</div>
			<p class="text-ink-400 text-sm">本章暂无内容</p>
			<p class="text-ink-600 text-xs">书籍可能尚未完成解析，请稍后刷新重试</p>
			<button
				type="button"
				onclick={() => readerStore.loadChapter(chapterIndex)}
				class="mt-2 px-4 py-2 rounded-lg bg-accent-500/10 text-accent-400 text-sm font-medium hover:bg-accent-500/20 transition-colors"
			>
				重新加载
			</button>
		</div>
	{:else if immersiveMode}
		<!-- Immersive bilingual translation mode (沉浸式翻译) -->
		<ImmersiveTranslation
			{content}
			{bookId}
			{chapterIndex}
		/>
	{:else}
		<!-- Chapter content with entity highlights -->
		{#key chapterIndex}
			<div
				id="reader-text"
				class="reader-text text-ink-200 selection:bg-accent-500/30"
				in:fade={{ duration: 200, delay: 50 }}
			>
				{@html renderedContent}
			</div>
		{/key}
	{/if}

	<!-- Context menu on text selection -->
	<SelectionContextMenu
		visible={contextMenuVisible}
		x={contextMenuX}
		y={contextMenuY}
		{selectedText}
		{bookId}
		{chapterIndex}
		onAnnotate={handleAnnotate}
		onClose={() => contextMenuVisible = false}
	/>

	<!-- Inline dictionary tooltip on double-click -->
	<DictionaryTooltip
		word={dictWord}
		position={dictPosition}
		visible={dictVisible}
		{bookId}
		onclose={() => dictVisible = false}
	/>
</div>

<style>
	.reader-text :global(.entity-highlight) {
		border-bottom: 2px dotted;
		cursor: pointer;
		transition: background-color 0.15s;
	}
	.reader-text :global(.entity-highlight:focus-visible) {
		outline: 2px solid rgba(var(--color-accent-500), 0.7);
		outline-offset: 2px;
		border-radius: 3px;
	}
	.reader-text :global(.entity-highlight:hover) {
		background-color: rgba(var(--color-accent-500), 0.15);
	}
	.reader-text :global(.entity-person) {
		border-color: var(--entity-person, #f59e0b);
	}
	.reader-text :global(.entity-location) {
		border-color: var(--entity-location, #10b981);
	}
	.reader-text :global(.entity-organization) {
		border-color: var(--entity-organization, #6366f1);
	}
	.reader-text :global(.entity-item) {
		border-color: var(--entity-item, #ec4899);
	}
	.reader-text :global(.entity-concept) {
		border-color: var(--entity-concept, #8b5cf6);
	}
</style>
