<script lang="ts">
	import ReaderContent from '$components/reader/ReaderContent.svelte';
	import ReaderSidebar from '$components/reader/ReaderSidebar.svelte';
	import ReaderToolbar from '$components/reader/ReaderToolbar.svelte';
	import ReadingSessionTracker from '$components/reader/ReadingSessionTracker.svelte';
	import SyncConflictDialog from '$components/reader/SyncConflictDialog.svelte';
	import TTSPlayer from '$components/reader/TTSPlayer.svelte';
	import { readerStore } from '$stores/reader.svelte';
	import { settingsStore } from '$stores/settings.svelte';
	import { createReaderShortcuts, shortcuts } from '$utils/shortcuts';
	import type { Bookmark } from '$types/models';
	import { toast } from 'svelte-sonner';
	import { ChevronLeft, ChevronRight, Maximize, Minimize, BookOpen, Columns2, PanelLeft } from 'lucide-svelte';

	import { page } from '$app/stores';
	import { browser } from '$app/environment';
	import { goto } from '$app/navigation';
	import { onMount, onDestroy, tick } from 'svelte';

	let bookId = $derived($page.params.id!);
	let chapterIndex = $state(0);
	let showSidebar = $state(false);
	let showSearch = $state(false);
	let showTTS = $state(false);
	let immersiveMode = $state(false);
	let searchQuery = $state('');
	let fullscreen = $state(false);
	let pageMode = $state<'scroll' | 'paginated'>('scroll');
	let columnMode = $state<'single' | 'dual'>('single');
	let scrollProgress = $state(0);
	let readerConfig = $state({
		fontSize: settingsStore.readerFontSize,
		lineHeight: settingsStore.readerLineHeight,
		fontFamily: settingsStore.readerFont,
		theme: 'dark',
		maxWidth: settingsStore.readerMaxWidth,
		highlightEntities: settingsStore.showEntityHighlights,
		textIndent: settingsStore.readerTextIndent,
		justify: settingsStore.readerJustify,
	});

	// Overall book progress: (chaptersCompleted + currentChapterProgress) / totalChapters
	let bookProgress = $derived(
		(readerStore.chapters?.length ?? 0) > 0
			? ((readerStore.currentChapterIndex + scrollProgress) / readerStore.chapters.length) * 100
			: 0
	);

	// Sync reader config changes back to persistent settings store
	$effect(() => {
		settingsStore.readerFontSize = readerConfig.fontSize;
		settingsStore.readerLineHeight = readerConfig.lineHeight;
		settingsStore.readerFont = readerConfig.fontFamily as import('$stores/settings.svelte').ReaderFont;
		settingsStore.readerMaxWidth = readerConfig.maxWidth;
		settingsStore.showEntityHighlights = readerConfig.highlightEntities;
	});

	let cleanupShortcuts: (() => void) | null = null;

	function scrollToContentOffset(targetOffset: number) {
		requestAnimationFrame(() => {
			setTimeout(() => {
				const contentLength = readerStore.content?.length || 1;
				const paragraphs = document.querySelectorAll<HTMLElement>('[data-char-offset]');
				let targetParagraph: HTMLElement | null = null;
				for (const p of paragraphs) {
					const offset = parseInt(p.dataset.charOffset || '0', 10);
					if (offset <= targetOffset) {
						targetParagraph = p;
					} else {
						break;
					}
				}

				if (targetParagraph) {
					targetParagraph.scrollIntoView({ behavior: 'smooth', block: 'center' });
					targetParagraph.style.backgroundColor = 'rgba(var(--color-accent-500), 0.15)';
					targetParagraph.style.borderRadius = '4px';
					targetParagraph.style.transition = 'background-color 2s ease-out';
					setTimeout(() => {
						if (targetParagraph) targetParagraph.style.backgroundColor = '';
					}, 3000);
				} else {
					const container = document.querySelector('[data-reader-content]') || document.documentElement;
					const ratio = Math.min(Math.max(targetOffset, 0) / contentLength, 0.95);
					container.scrollTo({ top: ratio * container.scrollHeight, behavior: 'smooth' });
				}
			}, 100);
		});
	}

	async function addBookmark() {
		if (!bookId) return;
		try {
			await readerStore.addBookmark({
				title: readerStore.currentChapter?.title ?? `第 ${readerStore.currentChapterIndex + 1} 章`,
				chapter_index: readerStore.currentChapterIndex,
				position: scrollProgress,
			});
			toast.success('已添加书签');
		} catch {
			toast.error('书签添加失败');
		}
	}

	onMount(async () => {
		if (browser) {
			immersiveMode = localStorage.getItem('nova_immersive_translation') === 'true';
		}

		if (bookId) {
			await readerStore.loadBook(bookId);

			// Honor ?chapter=X query param (from book detail page chapter list)
			const chapterParam = $page.url.searchParams.get('chapter');
			if (chapterParam !== null) {
				const targetChapter = parseInt(chapterParam, 10);
				if (!Number.isNaN(targetChapter) && targetChapter !== readerStore.currentChapterIndex) {
					await readerStore.loadChapter(targetChapter);
				}
			}

			// Honor ?chunk=X query param for precise scroll within chapter
			const chunkParam = $page.url.searchParams.get('chunk');
			if (chunkParam !== null) {
				const chunkIndex = parseInt(chunkParam, 10);
				if (!Number.isNaN(chunkIndex) && chunkIndex > 0) {
					const contentLength = readerStore.content?.length || 1;
					// Estimate: chunker uses 300-512 tokens, CJK ≈ 0.7 token/char → ~430-730 chars/chunk
					const estimatedChunkSize = Math.min(600, Math.max(400, contentLength / Math.ceil(contentLength / 600)));
					scrollToContentOffset(chunkIndex * estimatedChunkSize);
				}
			}

			const offsetParam = $page.url.searchParams.get('offset');
			if (offsetParam !== null && chunkParam === null) {
				const targetOffset = parseInt(offsetParam, 10);
				if (!Number.isNaN(targetOffset) && targetOffset >= 0) {
					scrollToContentOffset(targetOffset);
				}
			}
		}

		cleanupShortcuts = createReaderShortcuts({
			nextChapter: () => readerStore.nextChapter(),
			prevChapter: () => readerStore.prevChapter(),
			toggleSidebar: () => showSidebar = !showSidebar,
			toggleFullscreen: () => toggleFullscreen(),
			increaseFontSize: () => { readerConfig.fontSize = Math.min(32, readerConfig.fontSize + 1); },
			decreaseFontSize: () => { readerConfig.fontSize = Math.max(12, readerConfig.fontSize - 1); },
			bookmark: () => addBookmark(),
			search: () => { showSearch = true; },
		});

	});

	$effect(() => {
		if (browser) localStorage.setItem('nova_immersive_translation', String(immersiveMode));
	});

	onDestroy(() => {
		cleanupShortcuts?.();
		readerStore.cleanup();
	});

	// Bidirectional URL ↔ chapter state sync
	$effect(() => {
		const currentChapter = readerStore.currentChapterIndex;
		const urlChapter = $page.url.searchParams.get('chapter');
		const urlChapterNum = urlChapter !== null ? parseInt(urlChapter, 10) : null;

		// Only update URL if chapter changed and differs from current URL
		if (bookId && !readerStore.loading && (urlChapterNum === null || urlChapterNum !== currentChapter)) {
			const url = new URL($page.url);
			url.searchParams.set('chapter', String(currentChapter));
			url.searchParams.delete('chunk'); // clear chunk after initial navigation
			goto(url.toString(), { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	// Auto-save progress on page hide or before unload
	function handleVisibilityChange() {
		if (document.visibilityState === 'hidden') {
			readerStore.endSession();
		}
	}

	function handleBeforeUnload() {
		readerStore.endSession();
	}

	function toggleFullscreen() {
		if (!document.fullscreenElement) {
			document.documentElement.requestFullscreen().catch(() => {});
			fullscreen = true;
		} else {
			document.exitFullscreen().catch(() => {});
			fullscreen = false;
		}
	}

	function handleReaderScroll(e: Event) {
		const target = e.target as HTMLElement;
		const ratio = target.scrollTop / Math.max(1, target.scrollHeight - target.clientHeight);
		scrollProgress = ratio;
		readerStore.updateScroll(ratio);
	}

	// Scroll wheel at bottom → next chapter, at top → prev chapter
	let wheelAccumulator = $state(0);
	const WHEEL_THRESHOLD = 150; // pixels of overflow scroll needed to trigger

	function handleWheel(e: WheelEvent) {
		const container = document.getElementById('reader-scroll-container');
		if (!container) return;

		const atBottom = container.scrollTop >= container.scrollHeight - container.clientHeight - 2;
		const atTop = container.scrollTop <= 2;

		if (atBottom && e.deltaY > 0) {
			// Scrolling down at bottom
			wheelAccumulator += e.deltaY;
			if (wheelAccumulator > WHEEL_THRESHOLD) {
				wheelAccumulator = 0;
				if (readerStore.currentChapterIndex < (readerStore.chapters?.length ?? 0) - 1) {
					readerStore.nextChapter();
				}
			}
		} else if (atTop && e.deltaY < 0) {
			// Scrolling up at top
			wheelAccumulator += Math.abs(e.deltaY);
			if (wheelAccumulator > WHEEL_THRESHOLD) {
				wheelAccumulator = 0;
				if (readerStore.currentChapterIndex > 0) {
					readerStore.prevChapter();
				}
			}
		} else {
			wheelAccumulator = 0;
		}
	}

	// Restore scroll position when chapter content loads
	$effect(() => {
		if (!readerStore.loading && readerStore.scrollPosition > 0) {
			const container = document.getElementById('reader-scroll-container');
			if (container) {
				const targetScroll = readerStore.scrollPosition * (container.scrollHeight - container.clientHeight);
				container.scrollTo({ top: targetScroll, behavior: 'instant' });
			}
		}
	});

	let immersiveVisible = $state(true);
	let immersiveTimer: ReturnType<typeof setTimeout> | null = null;

	function handleImmersiveMouseMove() {
		immersiveVisible = true;
		if (immersiveTimer) clearTimeout(immersiveTimer);
		immersiveTimer = setTimeout(() => { immersiveVisible = false; }, 3000);
	}

	// Start auto-hide timer
	$effect(() => {
		// Auto-hide after 3s of inactivity
		immersiveTimer = setTimeout(() => { immersiveVisible = false; }, 3000);
		return () => { if (immersiveTimer) clearTimeout(immersiveTimer); };
	});

	function handleKeydown(e: KeyboardEvent) {
		shortcuts.handleKeydown(e);
		// Page mode: space/pagedown for next page
		if (pageMode === 'paginated') {
			const container = document.getElementById('reader-scroll-container');
			if (!container) return;
			if (e.key === ' ' || e.key === 'PageDown') {
				e.preventDefault();
				const atBottom = container.scrollTop >= container.scrollHeight - container.clientHeight - 2;
				if (atBottom) {
					readerStore.nextChapter();
				} else {
					container.scrollBy({ top: container.clientHeight - 60, behavior: 'smooth' });
				}
			} else if (e.key === 'PageUp') {
				e.preventDefault();
				const atTop = container.scrollTop <= 2;
				if (atTop) {
					readerStore.prevChapter();
				} else {
					container.scrollBy({ top: -(container.clientHeight - 60), behavior: 'smooth' });
				}
			}
		}
	}

	// Mobile swipe gesture for chapter navigation
	let touchStartX = 0;
	let touchStartY = 0;
	let swiping = false;

	function handleTouchStart(e: TouchEvent) {
		touchStartX = e.touches[0].clientX;
		touchStartY = e.touches[0].clientY;
		swiping = true;
	}

	function handleTouchEnd(e: TouchEvent) {
		if (!swiping) return;
		swiping = false;
		const deltaX = e.changedTouches[0].clientX - touchStartX;
		const deltaY = e.changedTouches[0].clientY - touchStartY;
		if (Math.abs(deltaX) > 80 && Math.abs(deltaX) > Math.abs(deltaY) * 1.5) {
			if (deltaX > 0) {
				readerStore.prevChapter();
			} else {
				readerStore.nextChapter();
			}
		}
	}

	// Jump to chapter from sidebar
	function handleChapterSelect(index: number) {
		readerStore.loadChapter(index);
		showSidebar = false;
	}

	async function handleBookmarkSelect(bookmark: Bookmark) {
		const targetChapter = bookmark.chapter_index ?? 0;
		const targetPosition = Math.max(0, Math.min(1, bookmark.position ?? 0));

		chapterIndex = targetChapter;
		if (targetChapter !== readerStore.currentChapterIndex) {
			await readerStore.loadChapter(targetChapter);
		}
		await tick();

		requestAnimationFrame(() => {
			const container = document.getElementById('reader-scroll-container');
			if (!container) return;
			const targetScroll = targetPosition * Math.max(0, container.scrollHeight - container.clientHeight);
			container.scrollTo({
				top: targetScroll,
				behavior: 'smooth',
			});
			scrollProgress = targetPosition;
			readerStore.updateScroll(targetPosition);
		});
		showSidebar = false;
	}
</script>

<svelte:head>
	<title>{readerStore.book?.title ?? 'Nova Reader'} — 阅读</title>
</svelte:head>

<svelte:window onkeydown={handleKeydown} onbeforeunload={handleBeforeUnload} />
<svelte:document onvisibilitychange={handleVisibilityChange} />

{#if bookId}
	<ReadingSessionTracker
		{bookId}
		chapterIndex={readerStore.currentChapterIndex}
		wordCount={readerStore.currentChapter?.word_count ?? 0}
	/>
{/if}

<SyncConflictDialog />

<div
	role="main"
	class="relative flex h-screen overflow-hidden bg-ink-950"
	onmousemove={handleImmersiveMouseMove}
>
	<!-- Reading progress bar (top, always visible) -->
	<div class="absolute top-0 left-0 right-0 z-50 h-[2px] bg-ink-900">
		<div
			class="h-full bg-accent-500/80 transition-all duration-300 ease-out"
			style="width: {bookProgress}%"
		></div>
	</div>

	<!-- Main content area -->
	<div class="flex flex-1 flex-col">
		<!-- Reader Toolbar (top, auto-hide) -->
		<div
			class="shrink-0 transition-all duration-300 ease-in-out"
			class:opacity-0={!immersiveVisible}
			class:pointer-events-none={!immersiveVisible}
			class:-translate-y-full={!immersiveVisible}
		>
			<ReaderToolbar
				bind:readerConfig
				bind:showSidebar
				bind:showTTS
				bind:immersiveMode
				{bookId}
				chapterIndex={readerStore.currentChapterIndex}
			/>
		</div>

		<!-- Main reading area -->
		<div
			class="flex-1 overflow-y-auto scroll-smooth"
			class:columns-2={columnMode === 'dual'}
			class:gap-12={columnMode === 'dual'}
			class:px-8={columnMode === 'dual'}
			id="reader-scroll-container"
			role="region"
			aria-label="阅读正文滚动区域"
			onscroll={handleReaderScroll}
			onwheel={handleWheel}
			ontouchstart={handleTouchStart}
			ontouchend={handleTouchEnd}
			data-reader-content
		>
			<ReaderContent
				{bookId}
				chapterIndex={readerStore.currentChapterIndex}
				config={readerConfig}
				bind:immersiveMode
				onprevchapter={() => readerStore.prevChapter()}
				onnextchapter={() => readerStore.nextChapter()}
			/>

			<!-- End of chapter: next chapter prompt -->
			{#if !readerStore.loading && readerStore.content}
				<div class="mx-auto max-w-2xl px-8 py-16 text-center">
					<div class="h-px w-24 mx-auto bg-ink-700/50 mb-8"></div>
					{#if readerStore.currentChapterIndex < (readerStore.chapters?.length ?? 0) - 1}
						<p class="text-xs text-ink-500 mb-3">下一章</p>
						<button
							onclick={() => readerStore.nextChapter()}
							class="text-sm text-ink-300 hover:text-accent-400 transition-colors"
						>
							{readerStore.chapters[readerStore.currentChapterIndex + 1]?.title ?? `第 ${readerStore.currentChapterIndex + 2} 章`}
						</button>
					{:else}
						<p class="text-sm text-ink-400">全书完</p>
					{/if}
				</div>
			{/if}
		</div>

		<!-- Bottom bar: chapter nav + progress (auto-hide) -->
		<div
			class="shrink-0 transition-all duration-300 ease-in-out border-t border-ink-800/30 bg-ink-950/95 backdrop-blur-sm"
			class:opacity-0={!immersiveVisible}
			class:pointer-events-none={!immersiveVisible}
			class:translate-y-full={!immersiveVisible}
		>
			<!-- Chapter progress slider -->
			<div class="px-6 pt-2">
				<input
					type="range"
					min="0"
					max={Math.max(1, (readerStore.chapters?.length ?? 1) - 1)}
					value={readerStore.currentChapterIndex}
					oninput={(e) => {
						const target = e.target as HTMLInputElement;
						readerStore.loadChapter(parseInt(target.value));
					}}
					class="w-full h-1 appearance-none bg-ink-800 rounded-full cursor-pointer accent-accent-500 [&::-webkit-slider-thumb]:h-3 [&::-webkit-slider-thumb]:w-3 [&::-webkit-slider-thumb]:appearance-none [&::-webkit-slider-thumb]:rounded-full [&::-webkit-slider-thumb]:bg-accent-400"
				/>
			</div>

			<!-- Nav buttons -->
			<div class="flex items-center justify-between px-4 py-2">
				<button
					onclick={() => readerStore.prevChapter()}
					disabled={readerStore.currentChapterIndex === 0}
					class="flex items-center gap-1 rounded-lg px-3 py-1.5 text-sm text-ink-400 hover:text-ink-200 hover:bg-ink-800/50 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
				>
					<ChevronLeft size={14} />
					上一章
				</button>

				<div class="flex items-center gap-3">
					<!-- Page mode toggle -->
					<button
						onclick={() => pageMode = pageMode === 'scroll' ? 'paginated' : 'scroll'}
						class="rounded-md p-1.5 text-ink-500 hover:text-ink-300 transition-colors"
						title={pageMode === 'scroll' ? '切换翻页模式' : '切换滚动模式'}
					>
						{#if pageMode === 'scroll'}
							<BookOpen size={14} />
						{:else}
							<Columns2 size={14} />
						{/if}
					</button>

					<!-- Column mode toggle (single/dual) -->
					<button
						onclick={() => columnMode = columnMode === 'single' ? 'dual' : 'single'}
						class="rounded-md p-1.5 transition-colors"
						class:text-accent-400={columnMode === 'dual'}
						class:text-ink-500={columnMode !== 'dual'}
						title={columnMode === 'single' ? '切换双栏模式' : '切换单栏模式'}
					>
						<PanelLeft size={14} />
					</button>

					<!-- Chapter indicator -->
					<span class="text-xs text-ink-500 tabular-nums">
						{readerStore.currentChapterIndex + 1} / {readerStore.chapters?.length ?? 0}
						<span class="ml-1 text-ink-600">·</span>
						<span class="ml-1">{Math.round(scrollProgress * 100)}%</span>
						{#if readerStore.content && scrollProgress < 1}
							{@const remainingChars = Math.round(readerStore.content.length * (1 - scrollProgress))}
							{@const minsLeft = Math.ceil(remainingChars / 800)}
							<span class="ml-1 text-ink-600">·</span>
							<span class="ml-1 text-ink-600">{minsLeft < 60 ? `~${minsLeft}分钟` : `~${(minsLeft / 60).toFixed(1)}h`}</span>
						{/if}
					</span>

					<!-- Fullscreen toggle -->
					<button
						onclick={toggleFullscreen}
						class="rounded-md p-1.5 text-ink-500 hover:text-ink-300 transition-colors"
						title={fullscreen ? '退出全屏' : '全屏'}
					>
						{#if fullscreen}
							<Minimize size={14} />
						{:else}
							<Maximize size={14} />
						{/if}
					</button>
				</div>

				<button
					onclick={() => readerStore.nextChapter()}
					disabled={readerStore.currentChapterIndex >= (readerStore.chapters?.length ?? 0) - 1}
					class="flex items-center gap-1 rounded-lg px-3 py-1.5 text-sm text-ink-400 hover:text-ink-200 hover:bg-ink-800/50 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
				>
					下一章
					<ChevronRight size={14} />
				</button>
			</div>
		</div>
	</div>

		<!-- TTS floating player -->
		{#if showTTS && readerStore.content}
			<div class="absolute bottom-20 left-1/2 -translate-x-1/2 z-40 w-[min(90vw,420px)]">
				<TTSPlayer text={readerStore.content} />
			</div>
		{/if}

		<!-- Right sidebar (chapters, annotations, entities) -->
		{#if showSidebar}
			<div class="absolute inset-y-0 right-0 z-40 w-80 shadow-2xl shadow-black/50 animate-in slide-in-from-right duration-200">
				<ReaderSidebar
				{bookId}
				bind:chapterIndex
				bind:immersiveMode
				onclose={() => showSidebar = false}
				onchapterselect={handleChapterSelect}
				onbookmarkselect={handleBookmarkSelect}
			/>
		</div>
		<!-- Backdrop -->
		<div
			role="presentation"
			class="absolute inset-0 z-30 bg-black/40"
			onclick={() => showSidebar = false}
			onkeydown={(e) => e.key === 'Escape' && (showSidebar = false)}
		></div>
	{/if}
</div>
