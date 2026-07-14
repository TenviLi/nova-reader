<script lang="ts">
	import { page } from '$app/stores';
	import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { queryKeys } from '$lib/queries';
	import { BookOpen, Edit3, FolderOpen, Star, Tag, ChevronRight, GripVertical, Network, Users } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';
	import { goto } from '$app/navigation';

	import type { Book } from '$types/models';

	let seriesId = $derived($page.params.id!);
	const queryClient = useQueryClient();

	const series = createQuery(() => ({
		queryKey: queryKeys.series.detail(seriesId),
		queryFn: () => api.getSeries(seriesId),
		enabled: !!seriesId,
	}));

	const seriesBooks = createQuery(() => ({
		queryKey: queryKeys.series.books(seriesId),
		queryFn: () => api.getSeriesBooks(seriesId),
		enabled: !!seriesId,
	}));

	let showEditMetadata = $state(false);
	let editGenres = $state('');
	let editTags = $state('');
	let editStatus = $state('unknown');
	let editRating = $state(0);

	// Drag reorder state
	let draggedIdx = $state<number | null>(null);
	let dragOverIdx = $state<number | null>(null);
	let reorderedBooks = $state<Book[] | null>(null);
	let activeTab = $state<'books' | 'graph' | 'characters'>('books');

	let displayBooks = $derived(reorderedBooks ?? seriesBooks.data ?? []);

	function handleDragStart(idx: number) {
		draggedIdx = idx;
	}

	function handleDragOver(e: DragEvent, idx: number) {
		e.preventDefault();
		dragOverIdx = idx;
	}

	function handleDrop(idx: number) {
		if (draggedIdx === null || draggedIdx === idx) {
			draggedIdx = null;
			dragOverIdx = null;
			return;
		}
		const items = [...displayBooks];
		const [moved] = items.splice(draggedIdx, 1);
		items.splice(idx, 0, moved);
		reorderedBooks = items;
		draggedIdx = null;
		dragOverIdx = null;

		// Save new order
		saveOrder(items.map((b) => b.id));
	}

	async function saveOrder(bookIds: string[]) {
		try {
			await api.reorderSeriesBooks(seriesId, bookIds);
			toast.success('排序已保存');
		} catch {
			// Endpoint may not exist — graceful fallback
			toast.info('排序已更新（本地）');
		}
	}

	function handleDragEnd() {
		draggedIdx = null;
		dragOverIdx = null;
	}

	function openEdit() {
		if (series.data) {
			editGenres = (series.data.metadata?.genres ?? []).join(', ');
			editTags = (series.data.metadata?.tags ?? []).join(', ');
			editStatus = series.data.status ?? 'unknown';
			editRating = series.data.metadata?.user_rating ?? 0;
		}
		showEditMetadata = true;
	}

	function formatWords(count: number): string {
		if (count >= 10000) return `${(count / 10000).toFixed(1)}万字`;
		if (count >= 1000) return `${(count / 1000).toFixed(0)}千字`;
		return `${count}字`;
	}

	const statusLabels: Record<string, string> = {
		ongoing: '连载中',
		completed: '已完结',
		hiatus: '暂停更新',
		cancelled: '已弃坑',
		unknown: '未知',
	};

	const statusColors: Record<string, string> = {
		ongoing: 'bg-emerald-500/10 text-emerald-400',
		completed: 'bg-blue-500/10 text-blue-400',
		hiatus: 'bg-amber-500/10 text-amber-400',
		cancelled: 'bg-red-500/10 text-red-400',
		unknown: 'bg-ink-500/10 text-ink-400',
	};
</script>

<svelte:head>
	<title>{series.data?.name ?? '系列详情'} — Nova Reader</title>
</svelte:head>

<div class="p-6 space-y-6 animate-fade-in">
	{#if series.isLoading}
		<div class="space-y-4">
			<div class="h-8 w-64 rounded-md bg-ink-800 animate-pulse"></div>
			<div class="h-4 w-96 rounded-md bg-ink-800 animate-pulse"></div>
		</div>
	{:else if series.data}
		{@const s = series.data}

		<!-- Header -->
		<div class="flex items-start gap-6">
			<!-- Cover -->
			<div class="shrink-0 w-32 h-44 rounded-lg bg-gradient-to-br from-amber-900/40 to-ink-900 border border-ink-700/50 flex items-center justify-center">
				{#if s.cover_path}
					<img src={s.cover_path} alt={s.name} class="w-full h-full object-cover rounded-lg" />
				{:else}
					<BookOpen class="w-10 h-10 text-ink-600" />
				{/if}
			</div>

			<!-- Info -->
			<div class="flex-1 min-w-0">
				<div class="flex items-center gap-3">
					<h1 class="text-2xl font-bold text-ink-50 truncate">{s.name}</h1>
					<span class="shrink-0 inline-flex items-center rounded-full px-2.5 py-0.5 text-xs font-medium {statusColors[s.status] ?? statusColors.unknown}">
						{statusLabels[s.status] ?? '未知'}
					</span>
				</div>

				{#if s.original_name && s.original_name !== s.name}
					<p class="mt-1 text-sm text-ink-400">{s.original_name}</p>
				{/if}

				<div class="mt-3 flex flex-wrap gap-4 text-sm text-ink-400">
					<span class="flex items-center gap-1">
						<BookOpen class="w-4 h-4" />
						{s.book_count} 本
					</span>
					<span class="flex items-center gap-1">
						{formatWords(s.total_word_count)}
					</span>
					<span class="flex items-center gap-1">
						<FolderOpen class="w-4 h-4" />
						<span class="truncate max-w-[200px]" title={s.folder_path}>{s.folder_path}</span>
					</span>
				</div>

				<!-- Rating -->
				{#if s.metadata?.user_rating}
					<div class="mt-2 flex items-center gap-1">
						{#each Array(5) as _, i}
							<Star
								class="w-4 h-4 {i < Math.round(s.metadata.user_rating / 2) ? 'text-amber-400 fill-amber-400' : 'text-ink-700'}"
							/>
						{/each}
						<span class="ml-1 text-sm text-ink-400">{s.metadata.user_rating.toFixed(1)}</span>
					</div>
				{/if}

				<!-- Genres & Tags -->
				{#if s.metadata?.genres?.length || s.metadata?.tags?.length}
					<div class="mt-3 flex flex-wrap gap-1.5">
						{#each s.metadata?.genres ?? [] as genre}
							<span class="inline-flex items-center rounded-md bg-amber-500/10 px-2 py-0.5 text-xs text-amber-300">
								{genre}
							</span>
						{/each}
						{#each s.metadata?.tags ?? [] as tag}
							<span class="inline-flex items-center rounded-md bg-ink-700/50 px-2 py-0.5 text-xs text-ink-300">
								<Tag class="w-3 h-3 mr-0.5" />
								{tag}
							</span>
						{/each}
					</div>
				{/if}

				<!-- Actions -->
				<div class="mt-4 flex gap-2">
					<button
						class="px-3 py-1.5 text-sm rounded-lg bg-amber-500/10 text-amber-300 hover:bg-amber-500/20 transition-colors"
						onclick={openEdit}
					>
						<Edit3 class="w-3.5 h-3.5 inline mr-1" />
						编辑元数据
					</button>
				</div>
			</div>
		</div>

		<!-- Description -->
		{#if s.metadata?.summary || s.description}
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-5">
				<h3 class="text-sm font-medium text-ink-300 mb-2">简介</h3>
				<p class="text-sm text-ink-200 leading-relaxed whitespace-pre-line">
					{s.metadata?.summary ?? s.description}
				</p>
			</div>
		{/if}

		<!-- Tab Navigation -->
		<div class="flex gap-1 border-b border-ink-800/50">
			{#each [
				{ key: 'books', label: '卷/册', icon: BookOpen },
				{ key: 'graph', label: '知识图谱', icon: Network },
				{ key: 'characters', label: '人物志', icon: Users },
			] as tab}
				<button
					onclick={() => activeTab = tab.key as typeof activeTab}
					class="flex items-center gap-1.5 px-4 py-2.5 text-sm font-medium border-b-2 -mb-[1px] transition-colors {activeTab === tab.key ? 'border-accent-500 text-accent-400' : 'border-transparent text-ink-400 hover:text-ink-200'}"
				>
					<tab.icon size={15} />
					{tab.label}
				</button>
			{/each}
		</div>

		<!-- Tab Content -->
		{#if activeTab === 'books'}
		<!-- Books in Series -->
		<div>
			<p class="text-xs text-ink-500 mb-4">拖拽排序</p>

			{#if displayBooks.length > 0}
				<div class="space-y-2" role="list" aria-label="系列卷册排序">
					{#each displayBooks as book, idx}
						<div
							role="listitem"
							aria-label="第 {idx + 1} 册：{book.volume_label ?? book.title}"
							draggable="true"
							ondragstart={() => handleDragStart(idx)}
							ondragover={(e) => handleDragOver(e, idx)}
							ondrop={() => handleDrop(idx)}
							ondragend={handleDragEnd}
							class="flex items-center gap-4 p-4 rounded-xl border transition-all text-left group cursor-grab active:cursor-grabbing
								{dragOverIdx === idx && draggedIdx !== idx ? 'border-accent-500/50 bg-accent-500/5' : 'border-ink-800/50 bg-ink-900/80 hover:border-amber-500/30 hover:bg-ink-900'}
								{draggedIdx === idx ? 'opacity-50' : ''}"
						>
							<!-- Drag handle -->
							<div class="shrink-0 text-ink-600 group-hover:text-ink-400 transition-colors">
								<GripVertical size={16} />
							</div>

							<!-- Volume number -->
							<div class="shrink-0 w-10 h-10 rounded-lg bg-ink-800 flex items-center justify-center">
								<span class="text-sm font-bold text-ink-300">{idx + 1}</span>
							</div>

							<!-- Book info -->
							<button
								class="flex-1 min-w-0 text-left"
								onclick={() => goto(`/library/${book.id}`)}
							>
								<p class="text-sm font-medium text-ink-100 truncate group-hover:text-accent-400 transition-colors">
									{book.volume_label ?? book.title}
								</p>
								<p class="text-xs text-ink-500 mt-0.5">
									{formatWords(book.word_count ?? 0)}
									{#if book.chapter_count > 0}
										· {book.chapter_count}章
									{/if}
								</p>
							</button>

							<!-- Reading progress -->
							{#if book.progress !== undefined && book.progress > 0}
								<div class="shrink-0 w-16">
									<div class="h-1.5 w-full rounded-full bg-ink-800">
										<div
											class="h-full rounded-full bg-amber-500"
											style="width: {book.progress * 100}%"
										></div>
									</div>
									<p class="text-xs text-ink-500 mt-0.5 text-right">
										{Math.round(book.progress * 100)}%
									</p>
								</div>
							{/if}

							<ChevronRight class="w-4 h-4 text-ink-600 group-hover:text-ink-400 transition-colors" />
						</div>
					{/each}
				</div>
			{:else}
				<p class="text-sm text-ink-500 text-center py-8">此系列暂无书籍</p>
			{/if}
		</div>

		{:else if activeTab === 'graph'}
			{#await import('$components/graph/LibraryGraphView.svelte') then { default: LibraryGraphView }}
				<LibraryGraphView libraryId={series.data?.library_id ?? ''} seriesId={seriesId} />
			{/await}

		{:else if activeTab === 'characters'}
			{#await import('$components/graph/LibraryCharactersView.svelte') then { default: LibraryCharactersView }}
				<LibraryCharactersView libraryId={series.data?.library_id ?? ''} seriesId={seriesId} />
			{/await}
		{/if}
	{:else if series.isError}
		<div class="text-center py-12 border border-red-900/50 rounded-xl bg-red-950/20">
			<p class="text-red-400 text-sm">加载系列失败：{series.error?.message ?? '未知错误'}</p>
			<button onclick={() => series.refetch()} class="mt-2 text-xs text-ink-400 hover:text-ink-200">重试</button>
		</div>
	{:else}
		<p class="text-ink-400">系列未找到</p>
	{/if}
</div>

<!-- Edit Metadata Modal -->
{#if showEditMetadata}
	<div class="fixed inset-0 z-50 flex items-center justify-center bg-black/60 backdrop-blur-sm">
		<div class="w-full max-w-md rounded-2xl border border-ink-700 bg-ink-900 p-6 shadow-2xl">
			<h3 class="text-lg font-semibold text-ink-100 mb-4">编辑系列元数据</h3>

			<div class="space-y-4">
				<div>
					<label for="series-edit-status" class="block text-sm text-ink-300 mb-1">状态</label>
					<select id="series-edit-status" bind:value={editStatus} class="w-full rounded-lg bg-ink-800 border border-ink-700 px-3 py-2 text-sm text-ink-100">
						<option value="ongoing">连载中</option>
						<option value="completed">已完结</option>
						<option value="hiatus">暂停更新</option>
						<option value="cancelled">已弃坑</option>
						<option value="unknown">未知</option>
					</select>
				</div>

				<div>
					<label for="series-edit-rating" class="block text-sm text-ink-300 mb-1">评分 (0-10)</label>
					<input
						id="series-edit-rating"
						type="number"
						min="0"
						max="10"
						step="0.5"
						bind:value={editRating}
						class="w-full rounded-lg bg-ink-800 border border-ink-700 px-3 py-2 text-sm text-ink-100"
					/>
				</div>

				<div>
					<label for="series-edit-genres" class="block text-sm text-ink-300 mb-1">题材 (逗号分隔)</label>
					<input
						id="series-edit-genres"
						type="text"
						bind:value={editGenres}
						placeholder="玄幻, 仙侠, 冒险"
						class="w-full rounded-lg bg-ink-800 border border-ink-700 px-3 py-2 text-sm text-ink-100"
					/>
				</div>

				<div>
					<label for="series-edit-tags" class="block text-sm text-ink-300 mb-1">标签 (逗号分隔)</label>
					<input
						id="series-edit-tags"
						type="text"
						bind:value={editTags}
						placeholder="爽文, 系统流, 重生"
						class="w-full rounded-lg bg-ink-800 border border-ink-700 px-3 py-2 text-sm text-ink-100"
					/>
				</div>
			</div>

			<div class="mt-6 flex justify-end gap-3">
				<button
					class="px-4 py-2 text-sm text-ink-400 hover:text-ink-200 transition-colors"
					onclick={() => showEditMetadata = false}
				>
					取消
				</button>
				<button
					class="px-4 py-2 text-sm rounded-lg bg-amber-500 text-ink-950 font-medium hover:bg-amber-400 transition-colors"
					onclick={async () => {
						try {
							await api.updateSeriesMetadata(seriesId, {
								status: editStatus,
								genres: editGenres.split(',').map(s => s.trim()).filter(Boolean),
								tags: editTags.split(',').map(s => s.trim()).filter(Boolean),
								user_rating: editRating,
							});
							toast.success('元数据已保存');
							showEditMetadata = false;
							queryClient.invalidateQueries({ queryKey: queryKeys.series.detail(seriesId) });
						} catch (err) {
							toast.error('保存失败', { description: err instanceof Error ? err.message : '请稍后重试' });
						}
					}}
				>
					保存
				</button>
			</div>
		</div>
	</div>
{/if}
