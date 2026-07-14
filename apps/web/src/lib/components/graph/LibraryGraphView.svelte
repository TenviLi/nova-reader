<script lang="ts">
	import { api } from '$services/api';
	import ForceGraph from './ForceGraph.svelte';
	import { Network, Maximize2, X, Filter } from 'lucide-svelte';
	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';

	let { libraryId, seriesId } = $props<{ libraryId?: string; seriesId?: string }>();

	let loading = $state(true);
	let graphFullscreen = $state(false);
	let filterType = $state<string | null>(null);
	let books = $state<Array<{ id: string; title: string }>>([]);
	let selectedBookId = $state<string | null>(null);

	let graphData = $state<{
		nodes: Array<{ id: string; label: string; type: string; size: number }>;
		edges: Array<{ source: string; target: string; label: string }>;
	}>({ nodes: [], edges: [] });

	const entityTypes = [
		{ value: null, label: '全部' },
		{ value: 'character', label: '人物' },
		{ value: 'location', label: '地点' },
		{ value: 'organization', label: '组织' },
		{ value: 'item', label: '物品' },
		{ value: 'event', label: '事件' },
	];

	async function loadData() {
		loading = true;
		try {
			const [graph, bookList] = await Promise.all([
				api.getEntityGraph({ book_id: selectedBookId ?? undefined, library_id: libraryId || undefined, series_id: seriesId || undefined }),
				books.length === 0
					? api.getBooks({ library_id: libraryId || undefined, series_id: seriesId || undefined }).then((r) => (r.data ?? []).map((b) => ({ id: b.id, title: b.title })))
					: Promise.resolve(books),
			]);
			graphData = graph;
			if (books.length === 0) books = bookList;
		} catch {
			graphData = { nodes: [], edges: [] };
		} finally {
			loading = false;
		}
	}

	function handleNodeClick(nodeId: string) {
		goto(`/characters/${nodeId}`);
	}

	onMount(loadData);

	$effect(() => {
		filterType; selectedBookId;
		loadData();
	});
</script>

<div
	class="flex flex-col overflow-hidden rounded-xl border border-ink-800/50"
	class:fixed={graphFullscreen} class:inset-0={graphFullscreen} class:z-50={graphFullscreen}
	class:bg-ink-950={graphFullscreen} class:h-[500px]={!graphFullscreen}
>
	<!-- Toolbar -->
	<div class="flex items-center justify-between border-b border-ink-800/50 bg-ink-900/50 px-4 py-2">
		<div class="flex items-center gap-2">
			<Network size={16} class="text-ink-400" />
			<span class="text-sm font-medium text-ink-200">实体关系图</span>
			<span class="text-xs text-ink-500">({graphData.nodes.length} 节点)</span>
		</div>
		<div class="flex items-center gap-2">
			<!-- Type filter chips -->
			<div class="flex gap-1">
				{#each entityTypes as t}
					<button
						onclick={() => filterType = t.value}
						class="px-2 py-0.5 rounded text-xs transition-colors {filterType === t.value ? 'bg-accent-500/20 text-accent-400' : 'text-ink-500 hover:text-ink-300'}"
					>{t.label}</button>
				{/each}
			</div>
			<!-- Book filter -->
			{#if books.length > 1}
				<select
					class="h-7 rounded bg-ink-800/50 border-ink-700/60 px-2 text-xs text-ink-300"
					onchange={(e) => selectedBookId = (e.target as HTMLSelectElement).value || null}
				>
					<option value="">全部书籍</option>
					{#each books as book}
						<option value={book.id}>{book.title}</option>
					{/each}
				</select>
			{/if}
			<button
				onclick={() => graphFullscreen = !graphFullscreen}
				class="rounded p-1 text-ink-500 hover:text-ink-200 hover:bg-ink-800/50"
			>
				{#if graphFullscreen}<X size={16} />{:else}<Maximize2 size={16} />{/if}
			</button>
		</div>
	</div>

	<!-- Graph -->
	<div class="flex-1 relative">
		{#if loading}
			<div class="absolute inset-0 flex items-center justify-center">
				<div class="h-8 w-8 rounded-full border-2 border-accent-500 border-t-transparent animate-spin"></div>
			</div>
		{:else if graphData.nodes.length === 0}
			<div class="absolute inset-0 flex flex-col items-center justify-center text-center">
				<Network size={36} class="text-ink-600 mb-2" />
				<p class="text-sm text-ink-400">暂无实体数据</p>
				<p class="text-xs text-ink-500 mt-1">对当前范围内的书籍执行 AI 分析后将生成知识图谱</p>
			</div>
		{:else}
			<ForceGraph
				nodes={graphData.nodes}
				edges={graphData.edges}
				onNodeClick={handleNodeClick}
			/>
		{/if}
	</div>
</div>
