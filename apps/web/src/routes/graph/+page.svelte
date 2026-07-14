<script lang="ts">
	import { api } from '$services/api';
	import ForceGraph from '$components/graph/ForceGraph.svelte';
	import EntityFlowGraph from '$components/graph/EntityFlowGraph.svelte';
	import { Network, Plus, Minus, Maximize2, X, BookOpen } from 'lucide-svelte';
	import { goto } from '$app/navigation';
	import { toast } from 'svelte-sonner';
	import { page } from '$app/stores';

	// URL-synced state
	let selectedEntity = $state<string | null>($page.url.searchParams.get('entity'));
	let filterType = $state<string | null>($page.url.searchParams.get('type'));
	let viewMode = $state<'graph' | 'list'>(
		($page.url.searchParams.get('view') as 'graph' | 'list') || 'graph'
	);
	let searchQuery = $state($page.url.searchParams.get('q') || '');
	let loading = $state(true);
	let selectedBookId = $state<string | null>($page.url.searchParams.get('book'));
	let errorMessage = $state<string | null>(null);
	let books = $state.raw<Array<{ id: string; title: string }>>([]);
	let graphFullscreen = $state(false);

	// Sync state back to URL
	$effect(() => {
		const params = new URLSearchParams();
		if (selectedEntity) params.set('entity', selectedEntity);
		if (filterType) params.set('type', filterType);
		if (viewMode !== 'graph') params.set('view', viewMode);
		if (searchQuery) params.set('q', searchQuery);
		if (selectedBookId) params.set('book', selectedBookId);
		const qs = params.toString();
		const newUrl = qs ? `/graph?${qs}` : '/graph';
		if (newUrl !== $page.url.pathname + $page.url.search) {
			goto(newUrl, { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	let entities = $state.raw<Array<{
		id: string;
		name: string;
		type: string;
		description: string;
		mention_count: number;
		book_count: number;
		relationships: number;
	}>>([]);

	let graphData = $state.raw<{
		nodes: Array<{ id: string; label: string; type: string; size: number }>;
		edges: Array<{ source: string; target: string; label: string }>;
	}>({ nodes: [], edges: [] });

	const entityTypes = [
		{ value: null, label: '全部', color: 'text-ink-300' },
		{ value: 'character', label: '人物', color: 'text-amber-400' },
		{ value: 'location', label: '地点', color: 'text-emerald-400' },
		{ value: 'organization', label: '组织', color: 'text-indigo-400' },
		{ value: 'item', label: '物品', color: 'text-pink-400' },
		{ value: 'skill', label: '技能', color: 'text-rose-400' },
		{ value: 'concept', label: '概念', color: 'text-violet-400' },
		{ value: 'event', label: '事件', color: 'text-cyan-400' },
	] as const;

	async function loadData() {
		loading = true;
		errorMessage = null;
		try {
			const [entList, graph, bookList] = await Promise.all([
				api.getEntities({ type: filterType ?? undefined, book_id: selectedBookId ?? undefined }),
				api.getEntityGraph({ book_id: selectedBookId ?? undefined }),
				books.length === 0 ? api.getBooks().then(r => r.data.map((b) => ({ id: b.id, title: b.title }))) : Promise.resolve(books),
			]);
			entities = entList.map(e => ({
				id: e.id,
				name: e.name,
				type: e.type,
				description: e.description ?? '',
				mention_count: e.mention_count ?? 0,
				book_count: e.book_count ?? 0,
				relationships: 0,
			}));
			graphData = graph;
			if (books.length === 0) books = bookList;
		} catch (err) {
			const message = err instanceof Error ? err.message : '加载图谱数据失败';
			errorMessage = message;
			// Only toast on non-initial loads (filter changes etc.)
			if (entities.length > 0) {
				toast.error('图谱加载失败', { description: message });
			}
			// Reset to empty state on error
			entities = [];
			graphData = { nodes: [], edges: [] };
		} finally {
			loading = false;
		}
	}

	function handleNodeClick(nodeId: string) {
		goto(`/characters/${nodeId}`);
	}

	$effect(() => {
		filterType; selectedBookId;
		loadData();
	});
</script>

<svelte:head>
	<title>Nova Reader — 知识图谱</title>
</svelte:head>

<div class="flex overflow-hidden animate-fade-in" class:h-[calc(100vh-4rem)]={!graphFullscreen} class:fixed={graphFullscreen} class:inset-0={graphFullscreen} class:z-50={graphFullscreen} class:h-screen={graphFullscreen} class:bg-ink-950={graphFullscreen}>
	<!-- Left: Graph canvas -->
	<div class="flex flex-1 flex-col">
		<!-- Toolbar -->
		<div class="flex items-center justify-between border-b border-ink-800/50 px-4 py-3">
			<div class="flex items-center gap-3">
				<h1 class="text-lg font-semibold text-ink-100">知识图谱</h1>
				<div class="flex gap-1 rounded-lg border border-ink-700/50 bg-ink-900/50 p-0.5">
					<button
						onclick={() => viewMode = 'graph'}
						class="rounded-md px-2.5 py-1 text-xs transition-colors"
						class:bg-ink-700={viewMode === 'graph'}
						class:text-ink-100={viewMode === 'graph'}
						class:text-ink-400={viewMode !== 'graph'}
					>
						图谱
					</button>
					<button
						onclick={() => viewMode = 'list'}
						class="rounded-md px-2.5 py-1 text-xs transition-colors"
						class:bg-ink-700={viewMode === 'list'}
						class:text-ink-100={viewMode === 'list'}
						class:text-ink-400={viewMode !== 'list'}
					>
						列表
					</button>
				</div>

				<!-- Book filter -->
				<select
					value={selectedBookId ?? ''}
					onchange={(e) => selectedBookId = (e.target as HTMLSelectElement).value || null}
					class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-2.5 py-1 text-xs text-ink-300 outline-none focus:border-accent-500/50"
				>
					<option value="">全部书籍</option>
					{#each books as book}
						<option value={book.id}>{book.title}</option>
					{/each}
				</select>
			</div>

			<!-- Type filters (chip toggle) -->
			<div class="flex gap-1.5 flex-wrap items-center">
				{#each entityTypes as type}
					<button
						onclick={() => filterType = type.value}
						class="rounded-full px-3 py-1 text-xs font-medium transition-all border {filterType === type.value ? 'border-accent-500/40 bg-accent-500/15 text-accent-300 shadow-sm shadow-accent-500/10' : 'border-ink-700/50 text-ink-400 hover:text-ink-200 hover:bg-ink-800/50'}"
					>
						<span class="inline-block w-2 h-2 rounded-full mr-1.5 {type.color.replace('text-', 'bg-')}" class:bg-ink-400={!type.value}></span>
						{type.label}
					</button>
				{/each}
				<button
					onclick={() => graphFullscreen = !graphFullscreen}
					class="ml-2 rounded-lg p-1.5 text-ink-400 hover:text-ink-100 hover:bg-ink-800/50 transition-colors"
					title={graphFullscreen ? '退出全屏' : '全屏'}
				>
					{#if graphFullscreen}
						<X size={16} />
					{:else}
						<Maximize2 size={16} />
					{/if}
				</button>
			</div>
		</div>

		<!-- Graph area or list -->
		{#if viewMode === 'graph'}
			<div class="flex-1 relative bg-ink-950">
				{#if loading}
					<div class="absolute inset-0 flex items-center justify-center">
						<div class="h-8 w-8 animate-spin rounded-full border-2 border-accent-500 border-t-transparent"></div>
					</div>
				{:else if graphData.nodes.length === 0}
					<div class="absolute inset-0 flex items-center justify-center">
						<div class="text-center">
							<div class="mb-4 flex h-20 w-20 mx-auto items-center justify-center rounded-2xl bg-ink-800/30 ring-1 ring-ink-700/30">
								<Network size={36} strokeWidth={1} class="text-ink-600" />
							</div>
							{#if errorMessage}
								<h3 class="text-lg font-medium text-red-400">加载失败</h3>
								<p class="mt-1 text-sm text-ink-500">{errorMessage}</p>
								<button
									onclick={() => loadData()}
									class="mt-3 rounded-lg bg-accent-600 px-4 py-2 text-sm text-white hover:bg-accent-500 transition-colors"
								>
									重试
								</button>
							{:else}
								<h3 class="text-lg font-medium text-ink-300">暂无图谱数据</h3>
								<p class="mt-1 text-sm text-ink-500">导入书籍并运行实体提取后，关系图谱将在此展示</p>
							{/if}
						</div>
					</div>
				{:else}
					<EntityFlowGraph
						nodes={graphData.nodes}
						edges={graphData.edges}
						onNodeClick={handleNodeClick}
					/>
				{/if}

				<!-- Zoom controls -->
				<div class="absolute bottom-4 right-4 flex flex-col gap-1.5">
					<button class="rounded-lg bg-ink-800/80 p-2 text-ink-300 hover:text-ink-100 backdrop-blur-sm transition-colors">
						<Plus size={16} strokeWidth={2} />
					</button>
					<button class="rounded-lg bg-ink-800/80 p-2 text-ink-300 hover:text-ink-100 backdrop-blur-sm transition-colors">
						<Minus size={16} strokeWidth={2} />
					</button>
					<button class="rounded-lg bg-ink-800/80 p-2 text-ink-300 hover:text-ink-100 backdrop-blur-sm transition-colors">
						<Maximize2 size={16} strokeWidth={2} />
					</button>
				</div>
			</div>
		{:else}
			<!-- List view -->
			<div class="flex-1 overflow-y-auto p-4">
				<div class="grid gap-3 md:grid-cols-2 xl:grid-cols-3">
					{#each entities as entity}
						<button
							onclick={() => selectedEntity = entity.id}
							class="flex items-start gap-3 rounded-xl border border-ink-800/50 bg-ink-900/30 p-4 text-left transition-all hover:border-ink-700/50 hover:bg-ink-800/30 {selectedEntity === entity.id ? 'border-accent-500/30' : ''}"
						>
							<div
								class="mt-0.5 h-3 w-3 shrink-0 rounded-full"
								class:bg-amber-400={entity.type === 'person'}
								class:bg-emerald-400={entity.type === 'location'}
								class:bg-indigo-400={entity.type === 'organization'}
								class:bg-pink-400={entity.type === 'item'}
								class:bg-violet-400={entity.type === 'concept'}
							></div>
							<div class="flex-1 overflow-hidden">
								<h4 class="font-medium text-ink-100">{entity.name}</h4>
								<p class="mt-1 text-xs text-ink-400 line-clamp-2">{entity.description}</p>
								<div class="mt-2 flex gap-3 text-[10px] text-ink-500">
									<span>出现 {entity.mention_count} 次</span>
									<span>涉及 {entity.book_count} 本书</span>
									<span>{entity.relationships} 关系</span>
								</div>
							</div>
						</button>
					{/each}
				</div>
			</div>
		{/if}
	</div>

	<!-- Right: Entity detail panel -->
	{#if selectedEntity}
		<aside class="w-80 shrink-0 overflow-y-auto border-l border-ink-800/50 bg-ink-950 p-4">
			<div class="flex items-center justify-between mb-4">
				<h3 class="font-semibold text-ink-100">实体详情</h3>
				<button onclick={() => selectedEntity = null} class="text-ink-400 hover:text-ink-100">
					<X size={16} strokeWidth={2} />
				</button>
			</div>
			<!-- Entity detail content would be rendered here -->
			<div class="text-sm text-ink-400">
				加载中...
			</div>
		</aside>
	{/if}
</div>
