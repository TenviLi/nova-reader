<script lang="ts">
	import { createMutation, createQuery, useQueryClient } from '@tanstack/svelte-query';
	import type { Collection, ReadingStatus, SmartShelf, SmartShelfBook } from '$types/models';
	import { toast } from 'svelte-sonner';
	import { Plus, Archive, BookOpen, Trash2, Sparkles, Filter, RefreshCw } from 'lucide-svelte';
	import { api } from '$services/api';
	import { useCollections, useCreateCollection, useDeleteCollection } from '$lib/queries';
	import * as Dialog from '$components/ui/dialog';
	import { Input } from '$components/ui/input';
	import { Button } from '$components/ui/button';
	import Skeleton from '$components/ui/Skeleton.svelte';

	const queryClient = useQueryClient();
	const collectionsQuery = useCollections();
	const createMut = useCreateCollection();
	const deleteMut = useDeleteCollection();

	const smartShelvesQuery = createQuery(() => ({
		queryKey: ['smart-shelves'],
		queryFn: () => api.getSmartShelves(),
	}));

	let activeTab = $state<'collections' | 'smart'>('collections');
	let collections = $derived<Collection[]>(collectionsQuery.data ?? []);
	let loading = $derived(collectionsQuery.isLoading);
	let showCreate = $state(false);
	let newName = $state('');
	let newDescription = $state('');
	let viewMode = $state<'grid' | 'list'>('grid');

	let selectedSmartShelfId = $state<string | null>(null);
	let showSmartForm = $state(false);
	let newSmartName = $state('');
	let newSmartDescription = $state('');
	let newSmartStatus = $state<ReadingStatus>('reading');

	const readingStatusOptions: Array<{ value: ReadingStatus; label: string; description: string }> = [
		{ value: 'unread', label: '未读', description: '还没有开始的书' },
		{ value: 'reading', label: '在读', description: '正在推进的书' },
		{ value: 'completed', label: '已读完', description: '适合回顾和整理' },
		{ value: 'on_hold', label: '搁置', description: '暂时放下的书' },
		{ value: 'dropped', label: '已弃读', description: '不再继续的书' },
	];

	$effect(() => {
		const shelves = smartShelvesQuery.data ?? [];
		if (!selectedSmartShelfId && shelves.length > 0) {
			selectedSmartShelfId = shelves[0].id;
		} else if (selectedSmartShelfId && shelves.length > 0 && !shelves.some((shelf) => shelf.id === selectedSmartShelfId)) {
			selectedSmartShelfId = shelves[0].id;
		}
	});

	let selectedSmartShelf = $derived(
		(smartShelvesQuery.data ?? []).find((shelf) => shelf.id === selectedSmartShelfId) ?? null
	);

	const smartShelfBooksQuery = createQuery(() => ({
		queryKey: ['smart-shelves', selectedSmartShelfId, 'books'],
		queryFn: () => api.getSmartShelfBooks(selectedSmartShelfId ?? ''),
		enabled: !!selectedSmartShelfId,
	}));

	const createSmartShelfMutation = createMutation(() => ({
		mutationFn: () => api.createSmartShelf({
			name: newSmartName.trim(),
			description: newSmartDescription.trim() || undefined,
			filter_criteria: { reading_status: newSmartStatus },
		}),
		onSuccess: (shelf) => {
			queryClient.invalidateQueries({ queryKey: ['smart-shelves'] });
			selectedSmartShelfId = shelf.id;
			newSmartName = '';
			newSmartDescription = '';
			newSmartStatus = 'reading';
			showSmartForm = false;
			toast.success('智能书架已创建');
		},
		onError: (err) => toast.error(err instanceof Error ? err.message : '创建失败'),
	}));

	async function createCollection() {
		if (!newName.trim()) return;
		createMut.mutate({ name: newName, description: newDescription || undefined });
		newName = '';
		newDescription = '';
		showCreate = false;
	}

	async function deleteCollection(id: string) {
		if (!confirm('确定删除此合集？书籍不会被删除。')) return;
		deleteMut.mutate(id);
	}

	function statusLabel(status: unknown): string {
		return readingStatusOptions.find((option) => option.value === status)?.label ?? '在读';
	}

	function smartShelfStatus(shelf: SmartShelf | null): ReadingStatus {
		const status = shelf?.filter_criteria?.reading_status;
		return readingStatusOptions.some((option) => option.value === status) ? status as ReadingStatus : 'reading';
	}

	function coverUrl(book: SmartShelfBook): string | null {
		return book.cover_path ? `/api/covers/${book.cover_path}` : null;
	}
</script>

<svelte:head>
	<title>书单与智能书架 — Nova Reader</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<div class="flex flex-col gap-4 lg:flex-row lg:items-center lg:justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">书单与智能书架</h1>
			<p class="mt-1 text-sm text-ink-400">手动整理主题书单，或让阅读状态自动生成动态书架。</p>
		</div>
		<div class="flex flex-wrap items-center gap-3">
			<div class="flex rounded-lg border border-ink-700/50 bg-ink-900/50 p-0.5" role="tablist" aria-label="组织方式">
				<button
					role="tab"
					aria-selected={activeTab === 'collections'}
					onclick={() => activeTab = 'collections'}
					class="rounded-md px-3 py-1.5 text-xs transition-colors {activeTab === 'collections' ? 'bg-ink-700 text-ink-100' : 'text-ink-400 hover:text-ink-200'}"
				>书单</button>
				<button
					role="tab"
					aria-selected={activeTab === 'smart'}
					onclick={() => activeTab = 'smart'}
					class="rounded-md px-3 py-1.5 text-xs transition-colors {activeTab === 'smart' ? 'bg-ink-700 text-ink-100' : 'text-ink-400 hover:text-ink-200'}"
				>智能书架</button>
			</div>
			{#if activeTab === 'collections'}
				<div class="flex gap-1 rounded-lg border border-ink-700/50 bg-ink-900/50 p-0.5">
					<button
						onclick={() => viewMode = 'grid'}
						class="rounded-md px-2.5 py-1 text-xs transition-colors {viewMode === 'grid' ? 'bg-ink-700 text-ink-100' : 'text-ink-400'}"
					>网格</button>
					<button
						onclick={() => viewMode = 'list'}
						class="rounded-md px-2.5 py-1 text-xs transition-colors {viewMode === 'list' ? 'bg-ink-700 text-ink-100' : 'text-ink-400'}"
					>列表</button>
				</div>
				<button
					onclick={() => showCreate = true}
					class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400"
				>
					<Plus size={16} strokeWidth={2} />
					新建书单
				</button>
			{:else}
				<button
					onclick={() => showSmartForm = !showSmartForm}
					class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400"
				>
					<Sparkles size={16} strokeWidth={2} />
					新建智能书架
				</button>
			{/if}
		</div>
	</div>

	{#if showCreate}
		<Dialog.Root bind:open={showCreate}>
			<Dialog.Content class="sm:max-w-md">
				<Dialog.Header>
					<Dialog.Title>创建新书单</Dialog.Title>
					<Dialog.Description>将书籍组织到主题书单中，便于浏览和回顾。</Dialog.Description>
				</Dialog.Header>
				<div class="space-y-4 py-4">
					<Input bind:value={newName} placeholder="书单名称" />
					<textarea
						bind:value={newDescription}
						placeholder="描述（可选）"
						rows="3"
						class="w-full resize-none rounded-lg border border-ink-700/50 bg-ink-900/80 px-3 py-2 text-sm text-ink-100 placeholder-ink-500 outline-none focus:border-accent-500/30"
					></textarea>
				</div>
				<Dialog.Footer>
					<Button variant="ghost" onclick={() => { showCreate = false; newName = ''; newDescription = ''; }}>取消</Button>
					<Button onclick={createCollection} disabled={!newName.trim()}>创建</Button>
				</Dialog.Footer>
			</Dialog.Content>
		</Dialog.Root>
	{/if}

	{#if activeTab === 'collections'}
		{#if loading}
			<div class="grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
				{#each Array(6) as _}
					<div class="space-y-4 rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
						<Skeleton class="h-24 w-full rounded-lg" />
						<Skeleton class="h-5 w-3/4" />
						<Skeleton class="h-4 w-1/2" />
					</div>
				{/each}
			</div>
		{:else if collections.length === 0}
			<div class="py-20 text-center">
				<div class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-ink-800/30 ring-1 ring-ink-700/30">
					<Archive size={28} strokeWidth={1.5} class="text-ink-500" />
				</div>
				<p class="mb-2 text-ink-300">还没有书单</p>
				<p class="text-sm text-ink-500">创建书单来组织你的书籍</p>
			</div>
		{:else}
			<div class={viewMode === 'grid' ? 'grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3' : 'space-y-3'}>
				{#each collections as collection (collection.id)}
					<a
						href="/collections/{collection.id}"
						class="group block rounded-xl border border-ink-800/50 bg-ink-900/30 p-5 transition-all hover:border-accent-500/30 hover:bg-ink-800/30"
					>
						<div class="mb-4 grid h-24 grid-cols-2 gap-0.5 overflow-hidden rounded-lg bg-ink-800/30">
							{#each Array(4) as _, i}
								{#if collection.book_ids && collection.book_ids[i]}
									<img
										src="/api/covers/{collection.book_ids[i]}"
										alt=""
										class="h-full w-full object-cover"
										onerror={(e) => { (e.target as HTMLImageElement).style.display = 'none'; (e.target as HTMLImageElement).nextElementSibling?.classList.remove('hidden'); }}
									/>
									<div class="hidden h-full w-full items-center justify-center bg-ink-800/50">
										<BookOpen size={16} strokeWidth={1.5} class="text-ink-600" />
									</div>
								{:else}
									<div class="flex items-center justify-center bg-ink-800/50">
										<BookOpen size={16} strokeWidth={1.5} class="text-ink-600" />
									</div>
								{/if}
							{/each}
						</div>

						<h3 class="truncate font-semibold text-ink-100 transition-colors group-hover:text-accent-400">
							{collection.name}
						</h3>
						{#if collection.description}
							<p class="mt-1 line-clamp-2 text-sm text-ink-400">{collection.description}</p>
						{/if}
						<div class="mt-3 flex items-center justify-between">
							<span class="text-xs text-ink-500">{collection.book_count} 本书</span>
							<button
								onclick={(e) => { e.preventDefault(); e.stopPropagation(); deleteCollection(collection.id); }}
								class="rounded p-1 text-ink-500 opacity-0 transition-all hover:text-error group-hover:opacity-100"
								aria-label="删除书单 {collection.name}"
							>
								<Trash2 size={16} strokeWidth={2} />
							</button>
						</div>
					</a>
				{/each}
			</div>
		{/if}
	{:else}
		{#if showSmartForm}
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<div class="grid gap-3 lg:grid-cols-[1fr_1.4fr_auto]">
					<div>
						<label class="mb-1 block text-xs text-ink-500" for="smart-shelf-name">书架名称</label>
						<input id="smart-shelf-name" bind:value={newSmartName} placeholder="例如：继续读" class="w-full rounded-lg border border-ink-800/70 bg-ink-950/40 px-3 py-2 text-sm text-ink-200 outline-none focus:border-accent-500/40" />
					</div>
					<div>
						<label class="mb-1 block text-xs text-ink-500" for="smart-shelf-desc">描述（可选）</label>
						<input id="smart-shelf-desc" bind:value={newSmartDescription} placeholder="给这个自动书架一个用途说明" class="w-full rounded-lg border border-ink-800/70 bg-ink-950/40 px-3 py-2 text-sm text-ink-200 outline-none focus:border-accent-500/40" />
					</div>
					<div class="flex items-end gap-2">
						<button
							onclick={() => createSmartShelfMutation.mutate()}
							disabled={!newSmartName.trim() || createSmartShelfMutation.isPending}
							class="rounded-lg bg-accent-500/20 px-3 py-2 text-sm font-medium text-accent-200 hover:bg-accent-500/30 disabled:opacity-50"
						>创建</button>
						<button onclick={() => showSmartForm = false} class="rounded-lg px-3 py-2 text-sm text-ink-400 hover:text-ink-200">取消</button>
					</div>
				</div>
				<div class="mt-4 grid gap-2 sm:grid-cols-2 lg:grid-cols-5">
					{#each readingStatusOptions as option}
						<button
							onclick={() => newSmartStatus = option.value}
							class="rounded-lg border px-3 py-2 text-left transition-colors {newSmartStatus === option.value ? 'border-accent-500/40 bg-accent-500/10 text-accent-200' : 'border-ink-800/60 bg-ink-950/20 text-ink-300 hover:border-ink-700'}"
							aria-pressed={newSmartStatus === option.value}
						>
							<div class="text-sm font-medium">{option.label}</div>
							<div class="mt-0.5 text-xs text-ink-500">{option.description}</div>
						</button>
					{/each}
				</div>
			</div>
		{/if}

		{#if smartShelvesQuery.isLoading}
			<div class="grid gap-4 lg:grid-cols-[320px_1fr]">
				<div class="space-y-3">
					{#each Array(3) as _}
						<div class="h-24 rounded-xl bg-ink-900/50 animate-pulse"></div>
					{/each}
				</div>
				<div class="h-80 rounded-xl bg-ink-900/40 animate-pulse"></div>
			</div>
		{:else if (smartShelvesQuery.data ?? []).length === 0}
			<div class="py-20 text-center">
				<div class="mx-auto mb-4 flex h-16 w-16 items-center justify-center rounded-full bg-accent-500/10 text-accent-400 ring-1 ring-accent-500/20">
					<Sparkles size={28} strokeWidth={1.5} />
				</div>
				<p class="mb-2 text-ink-300">还没有智能书架</p>
				<p class="text-sm text-ink-500">创建一个按阅读状态自动更新的书架</p>
			</div>
		{:else}
			<div class="grid gap-4 lg:grid-cols-[320px_1fr]">
				<div class="space-y-2">
					{#each smartShelvesQuery.data ?? [] as shelf}
						<button
							onclick={() => selectedSmartShelfId = shelf.id}
							class="w-full rounded-xl border p-4 text-left transition-colors {selectedSmartShelfId === shelf.id ? 'border-accent-500/35 bg-accent-500/10' : 'border-ink-800/50 bg-ink-900/30 hover:border-ink-700'}"
						>
							<div class="flex items-center gap-2">
								<Filter size={15} class={selectedSmartShelfId === shelf.id ? 'text-accent-300' : 'text-ink-500'} />
								<span class="truncate text-sm font-medium text-ink-100">{shelf.name}</span>
							</div>
							{#if shelf.description}
								<p class="mt-1 line-clamp-2 text-xs text-ink-500">{shelf.description}</p>
							{/if}
							<div class="mt-2 inline-flex rounded-full bg-ink-800/70 px-2 py-0.5 text-[11px] text-ink-300">
								{statusLabel(smartShelfStatus(shelf))}
							</div>
						</button>
					{/each}
				</div>

				<div class="rounded-xl border border-ink-800/50 bg-ink-900/25 p-4">
					<div class="mb-4 flex flex-wrap items-center justify-between gap-3">
						<div class="min-w-0">
							<h2 class="truncate text-lg font-semibold text-ink-100">{selectedSmartShelf?.name ?? '智能书架'}</h2>
							<p class="mt-1 text-xs text-ink-500">规则：阅读状态为 {statusLabel(smartShelfStatus(selectedSmartShelf))}</p>
						</div>
						<button
							onclick={() => queryClient.invalidateQueries({ queryKey: ['smart-shelves', selectedSmartShelfId, 'books'] })}
							class="inline-flex items-center gap-1.5 rounded-lg border border-ink-800/70 px-3 py-1.5 text-xs text-ink-300 hover:bg-ink-800/50"
						>
							<RefreshCw size={13} />
							刷新
						</button>
					</div>

					{#if smartShelfBooksQuery.isLoading}
						<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
							{#each Array(6) as _}
								<div class="h-24 rounded-lg bg-ink-950/40 animate-pulse"></div>
							{/each}
						</div>
					{:else if (smartShelfBooksQuery.data ?? []).length === 0}
						<div class="rounded-lg border border-dashed border-ink-800/70 px-4 py-12 text-center text-sm text-ink-500">
							当前没有匹配这条规则的书。
						</div>
					{:else}
						<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-3">
							{#each smartShelfBooksQuery.data ?? [] as book}
								<a href="/library/{book.id}" class="flex min-w-0 gap-3 rounded-lg border border-ink-800/50 bg-ink-950/30 p-3 transition-colors hover:border-accent-500/25 hover:bg-accent-500/5">
									{#if coverUrl(book)}
										<img src={coverUrl(book) ?? ''} alt={book.title} class="h-16 w-11 shrink-0 rounded object-cover" />
									{:else}
										<div class="flex h-16 w-11 shrink-0 items-center justify-center rounded bg-ink-800/60">
											<BookOpen size={15} class="text-ink-500" />
										</div>
									{/if}
									<div class="min-w-0">
										<div class="line-clamp-2 text-sm font-medium text-ink-100">{book.title}</div>
										{#if book.author}
											<div class="mt-1 truncate text-xs text-ink-500">{book.author}</div>
										{/if}
									</div>
								</a>
							{/each}
						</div>
					{/if}
				</div>
			</div>
		{/if}
	{/if}
</div>
