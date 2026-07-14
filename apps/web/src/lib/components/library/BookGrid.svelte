<script lang="ts">
	import type { Book } from '$types/models';
	import { api } from '$services/api';
	import { BookOpen } from 'lucide-svelte';
	import Checkbox from '$components/ui/checkbox/checkbox.svelte';
	import BookCard from './BookCard.svelte';
	import BookContextMenu from './BookContextMenu.svelte';
	import AddToCollectionDialog from './AddToCollectionDialog.svelte';

	let { viewMode, sortBy, filterStatus, filterLanguage, filterFormat = null, searchQuery, selectedIds = $bindable([]), visibleBookIds = $bindable([]), selectionMode = false } = $props<{
		viewMode: 'grid' | 'list' | 'table' | 'timeline';
		sortBy: string;
		filterStatus: string | null;
		filterLanguage: string | null;
		filterFormat?: string | null;
		searchQuery: string;
		selectedIds?: string[];
		visibleBookIds?: string[];
		selectionMode?: boolean;
	}>();

	// Books would be fetched from API
	let books = $state.raw<Book[]>([]);
	let loading = $state(true);
	let loadingMore = $state(false);
	let totalCount = $state(0);
	let currentPage = $state(1);
	let hasMore = $state(true);
	let sentinel = $state<HTMLDivElement | null>(null);
	let errorMessage = $state<string | null>(null);
	let requestSeq = 0;

	const PER_PAGE = 24;

	async function loadBooks(page: number) {
		const seq = ++requestSeq;
		if (page === 1) loading = true;
		else loadingMore = true;

		try {
			const result = await api.getBooks({
				page,
				per_page: PER_PAGE,
				sort_by: sortBy,
				status: filterStatus ?? undefined,
				language: filterLanguage ?? undefined,
				format: filterFormat ?? undefined,
				search: searchQuery || undefined,
			});
			if (seq !== requestSeq) return;
			if (page === 1) {
				books = result.data;
			} else {
				books = [...books, ...result.data];
			}
			errorMessage = null;
			totalCount = result.total;
			currentPage = page;
			hasMore = books.length < result.total;
		} catch {
			if (seq === requestSeq) {
				errorMessage = '书籍加载失败，请稍后重试';
			}
		} finally {
			if (seq === requestSeq) {
				loading = false;
				loadingMore = false;
			}
		}
	}

	$effect(() => {
		if (!sentinel || loading) return;
		const observer = new IntersectionObserver(
			(entries) => {
				if (entries[0].isIntersecting && hasMore && !loadingMore) {
					loadBooks(currentPage + 1);
				}
			},
			{ rootMargin: '200px' }
		);
		observer.observe(sentinel);
		return () => observer.disconnect();
	});

	// Reload when filters change
	$effect(() => {
		sortBy; filterStatus; filterLanguage; filterFormat; searchQuery;
		// Reset pagination state before loading to prevent stale infinite scroll triggers
		hasMore = true;
		currentPage = 1;
		loadBooks(1);
	});

	function toggleSelection(bookId: string) {
		if (selectedIds.includes(bookId)) {
			selectedIds = selectedIds.filter((id: string) => id !== bookId);
		} else {
			selectedIds = [...selectedIds, bookId];
		}
	}

	let visibleIds = $derived(books.map((book) => book.id));
	let allSelected = $derived(visibleIds.length > 0 && visibleIds.every((id) => selectedIds.includes(id)));

	$effect(() => {
		visibleBookIds = visibleIds;
	});

	function toggleSelectAll() {
		if (allSelected) {
			selectedIds = selectedIds.filter((id: string) => !visibleIds.includes(id));
		} else {
			selectedIds = Array.from(new Set([...selectedIds, ...visibleIds]));
		}
	}

	// Collection dialog state
	let collectionDialogOpen = $state(false);
	let collectionBookIds = $state<string[]>([]);

	function openCollectionDialog(bookId: string) {
		collectionBookIds = [bookId];
		collectionDialogOpen = true;
	}

	function handleBookDeleted(bookId: string) {
		books = books.filter(b => b.id !== bookId);
		selectedIds = selectedIds.filter((id: string) => id !== bookId);
		totalCount = Math.max(0, totalCount - 1);
	}

	function normalizeCoverPath(path: string | null | undefined): string | null {
		if (!path) return null;
		if (path.startsWith('/api/') || path.startsWith('http') || path.startsWith('data:')) return path;
		return `/api/covers/${path}`;
	}
</script>

{#if loading}
	<div data-testid="book-grid-loading" class="grid gap-4 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 2xl:grid-cols-6">
		{#each Array(12) as _}
			<div class="animate-pulse">
				<div class="aspect-[2/3] rounded-xl bg-ink-800/50"></div>
				<div class="mt-3 h-4 w-3/4 rounded bg-ink-800/50"></div>
				<div class="mt-1.5 h-3 w-1/2 rounded bg-ink-800/50"></div>
			</div>
		{/each}
	</div>
{:else if errorMessage}
	<div data-testid="book-grid-error" class="flex flex-col items-center justify-center rounded-xl border border-red-500/20 bg-red-500/5 py-16 text-center">
		<h3 class="text-lg font-semibold text-red-300">书籍加载失败</h3>
		<p class="mt-2 text-sm text-ink-400">{errorMessage}</p>
		<button
			class="mt-5 rounded-lg bg-ink-800 px-4 py-2 text-sm font-medium text-ink-100 hover:bg-ink-700 transition-colors"
			onclick={() => loadBooks(1)}
		>
			重试
		</button>
	</div>
{:else if books.length === 0}
	<div data-testid="book-grid-empty" class="flex flex-col items-center justify-center py-20 text-center">
		<div class="mb-6 flex h-24 w-24 items-center justify-center rounded-3xl bg-ink-800/30 ring-1 ring-ink-700/30">
			<BookOpen size={40} strokeWidth={1} class="text-ink-600" />
		</div>
		<h3 class="text-xl font-semibold text-ink-200">书架还空着</h3>
		<p class="mt-2 max-w-md text-sm text-ink-400">
			先添加书库文件夹，Nova Reader 会扫描并整理你的藏书
		</p>
		<div class="mt-6 flex justify-center">
			<a
				href="/libraries"
				class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-5 py-2.5 text-sm font-medium text-ink-950 hover:bg-accent-400 transition-colors"
			>
				添加书库
			</a>
		</div>
	</div>
{:else if viewMode === 'grid'}
	{#if selectionMode}
		<div class="flex items-center gap-2 mb-3">
				<Checkbox
					checked={allSelected}
					onCheckedChange={toggleSelectAll}
					aria-label="选择当前可见书籍"
					class="h-4 w-4"
				/>
			<span class="text-xs text-ink-400">全选 ({books.length})</span>
		</div>
	{/if}
	<div data-testid="book-grid" class="grid gap-4 sm:grid-cols-2 md:grid-cols-3 lg:grid-cols-4 xl:grid-cols-5 2xl:grid-cols-6">
		{#each books as book}
			<BookCard {book} {selectionMode} selected={selectedIds.includes(book.id)} onSelect={toggleSelection} onAddToCollection={openCollectionDialog} onDelete={handleBookDeleted} />
		{/each}
	</div>
{:else if viewMode === 'list'}
	<div data-testid="book-list" class="space-y-2">
		{#each books as book}
			<div class="group flex items-center gap-4 rounded-xl border border-ink-800/50 bg-ink-900/30 p-3 transition-all hover:border-ink-700/50 hover:bg-ink-800/30 {selectedIds.includes(book.id) ? 'ring-1 ring-accent-500/50' : ''}">
				{#if selectionMode}
					<div>
							<Checkbox
								checked={selectedIds.includes(book.id)}
								onCheckedChange={() => toggleSelection(book.id)}
								aria-label={`选择《${book.title}》`}
								class="h-4 w-4"
							/>
					</div>
				{/if}
				<a href="/library/{book.id}" class="flex flex-1 items-center gap-4">
					<div class="h-16 w-11 shrink-0 overflow-hidden rounded-lg bg-ink-800 shadow-sm">
						{#if book.cover_path}
							<img src={normalizeCoverPath(book.cover_path)} alt="" class="h-full w-full object-cover" loading="lazy" />
					{:else}
						<div class="flex h-full items-center justify-center text-[8px] text-ink-500">{book.title.slice(0, 2)}</div>
					{/if}
				</div>
				<div class="flex-1 overflow-hidden">
					<h4 class="truncate font-medium text-ink-100 group-hover:text-accent-400 transition-colors">{book.title}</h4>
					<p class="mt-0.5 text-sm text-ink-400">{book.author ?? '未知'} · {book.word_count ? `${Math.round(book.word_count / 10000)}万字` : '未知字数'}</p>
				</div>
				<div class="shrink-0 text-right">
					<div class="text-sm text-ink-300">{Math.round((book.progress ?? 0) * 100)}%</div>
					<div class="text-xs text-ink-500">{book.format?.toUpperCase()}</div>
				</div>
				</a>
				<div class="shrink-0 opacity-0 group-hover:opacity-100 transition-opacity">
					<BookContextMenu {book} onAddToCollection={openCollectionDialog} onDelete={handleBookDeleted} />
				</div>
			</div>
		{/each}
	</div>
{:else if viewMode === 'table'}
	<!-- Table view -->
	<div data-testid="book-table" class="overflow-x-auto rounded-xl border border-ink-800/50">
		<table class="w-full text-sm">
			<thead class="border-b border-ink-800/50 bg-ink-900/50">
				<tr>
					{#if selectionMode}
						<th class="w-10 px-3 py-3">
							<Checkbox
								checked={allSelected}
								onCheckedChange={toggleSelectAll}
								aria-label="选择当前表格页书籍"
								class="h-4 w-4"
							/>
						</th>
					{/if}
					<th class="px-4 py-3 text-left font-medium text-ink-300">标题</th>
					<th class="px-4 py-3 text-left font-medium text-ink-300">作者</th>
					<th class="px-4 py-3 text-left font-medium text-ink-300">字数</th>
					<th class="px-4 py-3 text-left font-medium text-ink-300">格式</th>
					<th class="px-4 py-3 text-left font-medium text-ink-300">进度</th>
					<th class="px-4 py-3 text-left font-medium text-ink-300">状态</th>
					<th class="w-10 px-2 py-3"></th>
				</tr>
			</thead>
			<tbody class="divide-y divide-ink-800/30">
				{#each books as book}
					<tr class="hover:bg-ink-800/20 transition-colors {selectedIds.includes(book.id) ? 'bg-accent-500/5' : ''}">
						{#if selectionMode}
							<td class="w-10 px-3 py-3">
									<Checkbox
										checked={selectedIds.includes(book.id)}
										onCheckedChange={() => toggleSelection(book.id)}
										aria-label={`选择《${book.title}》`}
										class="h-4 w-4"
									/>
							</td>
						{/if}
						<td class="px-4 py-3">
							<a href="/library/{book.id}" class="text-ink-100 hover:text-accent-400 transition-colors">{book.title}</a>
						</td>
						<td class="px-4 py-3 text-ink-400">{book.author ?? '—'}</td>
						<td class="px-4 py-3 text-ink-400">{book.word_count ? `${Math.round(book.word_count / 10000)}万` : '—'}</td>
						<td class="px-4 py-3 text-ink-400">{book.format?.toUpperCase() ?? '—'}</td>
						<td class="px-4 py-3">
							<div class="flex items-center gap-2">
								<div class="h-1.5 w-16 overflow-hidden rounded-full bg-ink-800">
									<div class="h-full rounded-full bg-accent-500" style="width: {(book.progress ?? 0) * 100}%"></div>
								</div>
								<span class="text-xs text-ink-400">{Math.round((book.progress ?? 0) * 100)}%</span>
							</div>
						</td>
						<td class="px-4 py-3">
							<span class="rounded-full px-2 py-0.5 text-xs {book.reading_status === 'reading' ? 'bg-accent-500/10 text-accent-400' : 'bg-ink-800 text-ink-400'}">
								{book.reading_status === 'reading' ? '在读' : book.reading_status === 'completed' ? '已读' : book.reading_status === 'on_hold' ? '搁置' : book.reading_status === 'dropped' ? '弃读' : '未读'}
							</span>
						</td>
						<td class="w-10 px-2 py-3">
							<BookContextMenu {book} onAddToCollection={openCollectionDialog} onDelete={handleBookDeleted} />
						</td>
					</tr>
				{/each}
			</tbody>
		</table>
	</div>
{:else if viewMode === 'timeline'}
	<!-- Timeline view: grouped by month -->
	{@const timelineGroups = (() => {
		const groups: Map<string, Book[]> = new Map();
		for (const book of books) {
			const date = book.created_at ? new Date(book.created_at) : new Date();
			const key = `${date.getFullYear()}年${date.getMonth() + 1}月`;
			if (!groups.has(key)) groups.set(key, []);
			groups.get(key)!.push(book);
		}
		return [...groups.entries()];
	})()}
	<div data-testid="book-timeline" class="relative pl-8">
		<!-- Timeline line -->
		<div class="absolute left-3 top-0 bottom-0 w-px bg-ink-800/60"></div>

		{#each timelineGroups as [month, monthBooks]}
			<!-- Month header -->
			<div class="relative mb-4">
				<div class="absolute -left-5 top-1 h-3 w-3 rounded-full bg-accent-500/20 ring-2 ring-accent-500/40"></div>
				<h3 class="text-sm font-semibold text-ink-200 mb-3">{month}</h3>
				<div class="space-y-2">
					{#each monthBooks as book}
						<a
							href="/library/{book.id}"
							class="group flex items-center gap-3 rounded-lg border border-ink-800/30 bg-ink-900/20 p-3 transition-all hover:border-ink-700/50 hover:bg-ink-800/30"
						>
								<div class="h-12 w-8 shrink-0 overflow-hidden rounded bg-ink-800 shadow-sm">
									{#if book.cover_path}
										<img src={normalizeCoverPath(book.cover_path)} alt="" class="h-full w-full object-cover" loading="lazy" />
								{:else}
									<div class="flex h-full items-center justify-center text-[7px] text-ink-500">{book.title.slice(0, 1)}</div>
								{/if}
							</div>
							<div class="flex-1 min-w-0">
								<h4 class="truncate text-sm font-medium text-ink-100 group-hover:text-accent-400 transition-colors">{book.title}</h4>
								<p class="text-xs text-ink-500">{book.author ?? '未知'} {#if book.word_count}· {Math.round(book.word_count / 10000)}万字{/if}</p>
							</div>
							<div class="shrink-0 text-xs text-ink-500">
								{#if book.created_at}
									{new Date(book.created_at).toLocaleDateString('zh-CN', { month: 'short', day: 'numeric' })}
								{/if}
							</div>
						</a>
					{/each}
				</div>
			</div>
		{/each}
	</div>
{/if}

<!-- Infinite scroll sentinel -->
{#if hasMore && !loading}
	<div bind:this={sentinel} class="flex justify-center py-8">
		{#if loadingMore}
			<div class="flex items-center gap-2 text-sm text-ink-400">
				<div class="h-4 w-4 animate-spin rounded-full border-2 border-accent-500 border-t-transparent"></div>
				加载更多...
			</div>
		{/if}
	</div>
{/if}

<!-- Add to Collection Dialog -->
<AddToCollectionDialog bind:open={collectionDialogOpen} bookIds={collectionBookIds} />

<!-- Total count -->
{#if !loading && books.length > 0}
	<div class="text-center text-xs text-ink-500 py-2">
		已加载 {books.length} / {totalCount} 本
	</div>
{/if}
