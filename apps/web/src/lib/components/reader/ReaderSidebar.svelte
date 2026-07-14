<script lang="ts">
	import { Bookmark as BookmarkIcon, Trash2, X } from 'lucide-svelte';
	import { readerStore } from '$stores/reader.svelte';
	import AnnotationShare from '$components/AnnotationShare.svelte';
	import ReadingCompanion from './ReadingCompanion.svelte';
	import ReaderIntelligencePanel from './ReaderIntelligencePanel.svelte';
	import { api } from '$services/api';
	import type { Bookmark } from '$types/models';
	import { toast } from 'svelte-sonner';
	import { tick } from 'svelte';

	let { bookId, chapterIndex = $bindable(), immersiveMode = $bindable(false), onclose, onchapterselect, onbookmarkselect } = $props<{
		bookId: string;
		chapterIndex: number;
		immersiveMode?: boolean;
		onclose: () => void;
		onchapterselect?: (index: number) => void;
		onbookmarkselect?: (bookmark: Bookmark) => void;
	}>();

	let activeTab = $state<'workspace' | 'chapters' | 'bookmarks' | 'annotations' | 'entities' | 'ai'>('workspace');
	let chapterListEl: HTMLDivElement | undefined = $state();
	let extractingEntities = $state(false);

	let chapters = $derived(readerStore.chapters ?? []);
	let annotations = $derived(readerStore.annotations ?? []);
	let bookmarks = $derived(readerStore.bookmarks ?? []);
	// Group inline entities by name with mention count
	let entities = $derived((() => {
		const raw = readerStore.entities ?? [];
		const map = new Map<string, { name: string; type: string; mentions: number; id: string }>();
		for (const e of raw) {
			const existing = map.get(e.name);
			if (existing) {
				existing.mentions++;
			} else {
				map.set(e.name, { name: e.name, type: e.type, mentions: 1, id: e.id });
			}
		}
		return [...map.values()].sort((a, b) => b.mentions - a.mentions);
	})());

	// Auto-scroll TOC to current chapter when sidebar opens or chapter changes
	$effect(() => {
		const idx = readerStore.currentChapterIndex;
		tick().then(() => {
			if (!chapterListEl) return;
			const active = chapterListEl.querySelector(`[data-chapter-idx="${idx}"]`);
			active?.scrollIntoView({ block: 'center', behavior: 'smooth' });
		});
	});

	async function runEntityExtraction() {
		if (extractingEntities) return;
		extractingEntities = true;
		try {
			await api.aiBatchProcess(bookId, ['entities']);
			await readerStore.loadChapter(readerStore.currentChapterIndex);
			toast.success('实体提取已完成');
		} catch (e) {
			toast.error(e instanceof Error ? e.message : '实体提取失败');
		} finally {
			extractingEntities = false;
		}
	}

	function bookmarkTitle(bookmark: Bookmark): string {
		return bookmark.title?.trim() || `第 ${bookmarkChapter(bookmark) + 1} 章`;
	}

	function bookmarkChapter(bookmark: Bookmark): number {
		return bookmark.chapter_index ?? 0;
	}

	function bookmarkPosition(bookmark: Bookmark): string {
		return `${Math.round((bookmark.position ?? 0) * 100)}%`;
	}

	function bookmarkCreatedAt(bookmark: Bookmark): string {
		const date = new Date(bookmark.created_at);
		if (Number.isNaN(date.getTime())) return '';
		return new Intl.DateTimeFormat('zh-CN', {
			month: '2-digit',
			day: '2-digit',
			hour: '2-digit',
			minute: '2-digit',
		}).format(date);
	}

	async function handleDeleteBookmark(bookmarkId: string) {
		try {
			await readerStore.deleteBookmark(bookmarkId);
			toast.success('已删除书签');
		} catch {
			toast.error('删除书签失败');
		}
	}
</script>

<aside class="flex w-80 shrink-0 flex-col border-l border-ink-800/50 bg-ink-950">
	<!-- Header -->
	<div class="flex items-center justify-between border-b border-ink-800/50 px-4 py-3">
		<!-- Tabs -->
		<div class="flex flex-wrap gap-1">
			<button
				onclick={() => activeTab = 'workspace'}
				class="rounded-md px-2.5 py-1 text-xs font-medium transition-colors"
				class:bg-ink-800={activeTab === 'workspace'}
				class:text-ink-100={activeTab === 'workspace'}
				class:text-ink-400={activeTab !== 'workspace'}
			>
				工作台
			</button>
			<button
				onclick={() => activeTab = 'chapters'}
				class="rounded-md px-2.5 py-1 text-xs font-medium transition-colors"
				class:bg-ink-800={activeTab === 'chapters'}
				class:text-ink-100={activeTab === 'chapters'}
				class:text-ink-400={activeTab !== 'chapters'}
			>
				目录
			</button>
			<button
				onclick={() => activeTab = 'bookmarks'}
				class="rounded-md px-2.5 py-1 text-xs font-medium transition-colors"
				class:bg-ink-800={activeTab === 'bookmarks'}
				class:text-ink-100={activeTab === 'bookmarks'}
				class:text-ink-400={activeTab !== 'bookmarks'}
			>
				书签
			</button>
			<button
				onclick={() => activeTab = 'annotations'}
				class="rounded-md px-2.5 py-1 text-xs font-medium transition-colors"
				class:bg-ink-800={activeTab === 'annotations'}
				class:text-ink-100={activeTab === 'annotations'}
				class:text-ink-400={activeTab !== 'annotations'}
			>
				批注
			</button>
			<button
				onclick={() => activeTab = 'entities'}
				class="rounded-md px-2.5 py-1 text-xs font-medium transition-colors"
				class:bg-ink-800={activeTab === 'entities'}
				class:text-ink-100={activeTab === 'entities'}
				class:text-ink-400={activeTab !== 'entities'}
			>
				实体
			</button>
			<button
				onclick={() => activeTab = 'ai'}
				class="rounded-md px-2.5 py-1 text-xs font-medium transition-colors"
				class:bg-ink-800={activeTab === 'ai'}
				class:text-ink-100={activeTab === 'ai'}
				class:text-ink-400={activeTab !== 'ai'}
			>
				AI
			</button>
		</div>

		<button onclick={onclose} class="rounded-md p-1 text-ink-400 hover:text-ink-100 transition-colors">
			<X size={16} strokeWidth={2} />
		</button>
	</div>

	<!-- Content -->
	<div class="flex-1 overflow-y-auto p-3">
		{#if activeTab === 'workspace'}
			<ReaderIntelligencePanel
				{bookId}
				bookTitle={readerStore.book?.title ?? ''}
				chapterIndex={readerStore.currentChapterIndex}
				totalChapters={readerStore.chapters?.length ?? 0}
				content={readerStore.content}
				entityCount={entities.length}
				annotationCount={annotations.length}
				bind:immersiveMode
			/>

		{:else if activeTab === 'chapters'}
			<div class="space-y-0.5" bind:this={chapterListEl}>
				{#each chapters as chapter, i}
					<button
						data-chapter-idx={i}
						onclick={() => { chapterIndex = i; onchapterselect?.(i); }}
						class="flex w-full items-center gap-3 rounded-lg px-3 py-2 text-left transition-colors {i === readerStore.currentChapterIndex ? 'bg-accent-500/10 text-accent-400' : 'text-ink-300 hover:bg-ink-800/50'}"
					>
						<span class="shrink-0 text-xs text-ink-500 w-6 text-right">{i + 1}</span>
						<span class="flex-1 truncate text-sm">{chapter.title}</span>
						<span class="shrink-0 text-[10px] text-ink-500">
							{Math.round(chapter.word_count / 1000)}k
						</span>
					</button>
				{/each}
			</div>

		{:else if activeTab === 'bookmarks'}
			{#if bookmarks.length === 0}
				<div class="py-8 text-center text-sm text-ink-500">
					<BookmarkIcon size={28} class="mx-auto mb-3 opacity-40" />
					<p>还没有书签</p>
				</div>
			{:else}
				<div class="space-y-2">
					{#each bookmarks as bookmark}
						<div class="flex items-stretch gap-2 rounded-lg border border-ink-800/50 bg-ink-900/40 p-2">
							<button
								onclick={() => onbookmarkselect?.(bookmark)}
								class="min-w-0 flex-1 rounded-md px-2 py-1.5 text-left transition-colors hover:bg-ink-800/60 focus:outline-none focus:ring-2 focus:ring-accent-500/50"
							>
								<span class="block truncate text-sm font-medium text-ink-200">{bookmarkTitle(bookmark)}</span>
								<span class="mt-1 flex items-center gap-2 text-[11px] text-ink-500">
									<span>第 {bookmarkChapter(bookmark) + 1} 章</span>
									<span>·</span>
									<span>{bookmarkPosition(bookmark)}</span>
									{#if bookmarkCreatedAt(bookmark)}
										<span>·</span>
										<span>{bookmarkCreatedAt(bookmark)}</span>
									{/if}
								</span>
							</button>
							<button
								onclick={() => handleDeleteBookmark(bookmark.id)}
								aria-label={`删除书签 ${bookmark.title ?? bookmarkTitle(bookmark)}`}
								class="grid h-9 w-9 shrink-0 place-items-center self-center rounded-md text-ink-500 transition-colors hover:bg-red-500/10 hover:text-red-400 focus:outline-none focus:ring-2 focus:ring-red-500/40"
							>
								<Trash2 size={14} />
							</button>
						</div>
					{/each}
				</div>
			{/if}

		{:else if activeTab === 'annotations'}
			{#if annotations.length === 0}
				<div class="py-8 text-center text-sm text-ink-500">
					选中文本即可添加批注
				</div>
			{:else}
				<div class="space-y-3">
					{#each annotations as ann}
						<div class="rounded-lg border border-ink-800/50 bg-ink-900/50 p-3">
							<div class="mb-1 flex items-center gap-2">
								<div class="h-2 w-2 rounded-full" style="background: {ann.color}"></div>
								<span class="text-[10px] text-ink-500">第 {ann.chapter_index + 1} 章</span>
							</div>
							<p class="text-xs text-ink-200 italic border-l-2 border-ink-700 pl-2 mb-2">"{ann.selected_text}"</p>
							{#if ann.note}
								<p class="text-xs text-ink-400">{ann.note}</p>
							{/if}
							<div class="mt-3 border-t border-ink-800/60 pt-3">
								<AnnotationShare
									annotationId={ann.id}
									bookTitle={readerStore.book?.title ?? ''}
									text={ann.selected_text}
									note={ann.note ?? ''}
									showPreview={false}
								/>
							</div>
						</div>
					{/each}
				</div>
			{/if}

		{:else if activeTab === 'entities'}
			{#if entities.length === 0}
					<div class="py-8 text-center text-sm text-ink-500">
						<p>尚未提取实体</p>
						<button
							class="mt-2 text-accent-400 hover:text-accent-300 text-xs disabled:cursor-not-allowed disabled:opacity-50"
							onclick={runEntityExtraction}
							disabled={extractingEntities}
						>
							{extractingEntities ? '提取中...' : '运行实体提取'}
						</button>
					</div>
			{:else}
				<div class="space-y-1">
					{#each entities as entity}
						<a
							href="/characters/{entity.id}"
							class="flex items-center gap-3 rounded-lg px-3 py-2 text-sm text-ink-300 hover:bg-ink-800/50 hover:text-ink-100 transition-colors"
						>
							<span
								class="h-2 w-2 shrink-0 rounded-full"
								class:bg-amber-400={entity.type === 'person'}
								class:bg-emerald-400={entity.type === 'location'}
								class:bg-indigo-400={entity.type === 'organization'}
								class:bg-pink-400={entity.type === 'item'}
								class:bg-violet-400={entity.type === 'concept'}
							></span>
							<span class="flex-1 truncate">{entity.name}</span>
							<span class="text-[10px] text-ink-500">{entity.mentions}次</span>
						</a>
					{/each}
				</div>
			{/if}

		{:else if activeTab === 'ai'}
			<ReadingCompanion
				{bookId}
				bookTitle={readerStore.book?.title ?? ''}
				currentChapter={readerStore.currentChapterIndex}
				totalChapters={readerStore.chapters?.length ?? 0}
			/>
		{/if}
	</div>
</aside>
