<script lang="ts">
	import BookGrid from '$components/library/BookGrid.svelte';
	import LibraryFilters from '$components/library/LibraryFilters.svelte';
	import LibraryHeader from '$components/library/LibraryHeader.svelte';
	import AddToCollectionDialog from '$components/library/AddToCollectionDialog.svelte';
	import { CheckSquare, X, FolderPlus } from 'lucide-svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';

	// URL-synced filter state
	let viewMode = $state<'grid' | 'list' | 'table' | 'timeline'>(
		($page.url.searchParams.get('view') as 'grid' | 'list' | 'table' | 'timeline') || 'grid'
	);
	let sortBy = $state($page.url.searchParams.get('sort') || 'updated_at');
	let filterStatus = $state<string | null>($page.url.searchParams.get('status'));
	let filterLanguage = $state<string | null>($page.url.searchParams.get('lang'));
	let filterFormat = $state<string | null>($page.url.searchParams.get('format'));
	let searchQuery = $state($page.url.searchParams.get('q') || '');

	// Selection mode
	let selectionMode = $state(false);
	let selectedIds = $state<string[]>([]);
	let visibleBookIds = $state<string[]>([]);
	let batchCollectionOpen = $state(false);
	let allVisibleSelected = $derived(
		visibleBookIds.length > 0 && visibleBookIds.every((id) => selectedIds.includes(id))
	);

	// Sync state changes to URL
	$effect(() => {
		const params = new URLSearchParams();
		if (viewMode !== 'grid') params.set('view', viewMode);
		if (sortBy !== 'updated_at') params.set('sort', sortBy);
		if (filterStatus) params.set('status', filterStatus);
		if (filterLanguage) params.set('lang', filterLanguage);
		if (filterFormat) params.set('format', filterFormat);
		if (searchQuery) params.set('q', searchQuery);
		const qs = params.toString();
		const newUrl = qs ? `/library?${qs}` : '/library';
		if (newUrl !== $page.url.pathname + $page.url.search) {
			goto(newUrl, { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	function toggleSelectAllVisible() {
		if (allVisibleSelected) {
			selectedIds = selectedIds.filter((id) => !visibleBookIds.includes(id));
		} else {
			selectedIds = Array.from(new Set([...selectedIds, ...visibleBookIds]));
		}
	}
</script>

<svelte:head>
	<title>Nova Reader — 所有书籍</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in relative">
	<div>
		<div class="flex items-center justify-between">
			<LibraryHeader bind:viewMode bind:searchQuery />
			<button
				type="button"
				onclick={() => { selectionMode = !selectionMode; if (!selectionMode) selectedIds = []; }}
				aria-pressed={selectionMode}
				class="rounded-lg px-3 py-1.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {selectionMode ? 'bg-accent-500/10 text-accent-400 ring-1 ring-accent-500/30' : 'text-ink-400 hover:text-ink-200 hover:bg-ink-800/50'}"
			>
				<CheckSquare size={14} class="inline mr-1" />
				{selectionMode ? '退出选择' : '批量操作'}
			</button>
		</div>

		<!-- Selection toolbar (shown when in selection mode) -->
		{#if selectionMode}
			<div class="mt-3 flex items-center gap-3 rounded-xl border border-accent-500/20 bg-accent-500/5 px-4 py-2.5">
				<label class="flex items-center gap-2 cursor-pointer">
					<input
						type="checkbox"
						checked={allVisibleSelected}
						disabled={visibleBookIds.length === 0}
						onchange={toggleSelectAllVisible}
						class="rounded border-ink-700 bg-ink-800 text-accent-500 focus:ring-accent-500 h-4 w-4"
					/>
					<span class="text-xs text-ink-400">全选</span>
				</label>
				{#if selectedIds.length > 0}
					<span class="text-sm font-medium text-accent-400">{selectedIds.length} 项已选</span>
					<div class="h-4 w-px bg-ink-700"></div>
					<button
						type="button"
						onclick={() => batchCollectionOpen = true}
						class="flex items-center gap-1.5 rounded-md px-2.5 py-1 text-xs text-ink-300 transition-colors hover:bg-ink-800/50 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
					>
						<FolderPlus size={12} /> 添加到合集
					</button>
					<button
						type="button"
						onclick={() => { selectedIds = []; }}
						class="ml-auto flex items-center gap-1 rounded-md px-2 py-1 text-xs text-ink-400 transition-colors hover:text-ink-200 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
					>
						<X size={12} /> 清除选择
					</button>
				{/if}
			</div>
		{/if}

		<div class="mt-4">
			<LibraryFilters bind:sortBy bind:filterStatus bind:filterLanguage bind:filterFormat />
		</div>

		<!-- Quick filter chips -->
		<div class="mt-3 flex flex-wrap items-center gap-2">
			<span class="text-xs text-ink-500 mr-1">格式:</span>
			{#each ['epub', 'txt', 'pdf', 'docx', 'doc', 'md', 'html'] as fmt}
				<button
					type="button"
					onclick={() => filterFormat = filterFormat === fmt ? null : fmt}
					aria-pressed={filterFormat === fmt}
					class="rounded-full px-2.5 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {filterFormat === fmt ? 'bg-accent-500/20 text-accent-300 ring-1 ring-accent-500/30' : 'bg-ink-800/50 text-ink-400 hover:bg-ink-800 hover:text-ink-200'}"
				>{fmt.toUpperCase()}</button>
			{/each}
			<span class="text-ink-700 mx-1">|</span>
			<span class="text-xs text-ink-500 mr-1">状态:</span>
			{#each [{v:'reading',l:'在读'},{v:'unread',l:'未读'},{v:'completed',l:'已读'}] as {v, l}}
				<button
					type="button"
					onclick={() => filterStatus = filterStatus === v ? null : v}
					aria-pressed={filterStatus === v}
					class="rounded-full px-2.5 py-1 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {filterStatus === v ? 'bg-accent-500/20 text-accent-300 ring-1 ring-accent-500/30' : 'bg-ink-800/50 text-ink-400 hover:bg-ink-800 hover:text-ink-200'}"
				>{l}</button>
			{/each}
		</div>

		<div class="mt-4">
			<BookGrid {viewMode} {sortBy} {filterStatus} {filterLanguage} {filterFormat} {searchQuery} {selectionMode} bind:selectedIds bind:visibleBookIds />
		</div>
	</div>
</div>

<AddToCollectionDialog bind:open={batchCollectionOpen} bookIds={selectedIds} />
