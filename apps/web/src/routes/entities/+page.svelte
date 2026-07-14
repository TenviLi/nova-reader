<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { Users, Search, Filter, ChevronLeft, ChevronRight, BookOpen, Network } from 'lucide-svelte';

	interface Entity {
		id: string;
		name: string;
		entity_type: string;
		description: string | null;
		aliases: string[];
		mention_count: number;
		importance_score: number;
		created_at: string;
	}

	let entities = $state<Entity[]>([]);
	let loading = $state(true);
	let search = $state('');
	let selectedType = $state<string | null>(null);
	let selectedBookId = $state<string | null>(null);
	let books = $state<Array<{ id: string; title: string }>>([]);
	let offset = $state(0);
	let hasMore = $state(false);
	const limit = 50;

	const entityTypes = ['character', 'location', 'organization', 'item', 'event', 'concept'];
	const typeColors: Record<string, string> = {
		character: 'bg-blue-500/10 text-blue-400 border-blue-500/20',
		location: 'bg-green-500/10 text-green-400 border-green-500/20',
		organization: 'bg-purple-500/10 text-purple-400 border-purple-500/20',
		item: 'bg-amber-500/10 text-amber-400 border-amber-500/20',
		event: 'bg-rose-500/10 text-rose-400 border-rose-500/20',
		concept: 'bg-cyan-500/10 text-cyan-400 border-cyan-500/20',
	};

	onMount(async () => {
		try {
			const result = await api.getBooks();
			books = result.data?.map((b) => ({ id: b.id, title: b.title })) ?? [];
		} catch { /* ignore */ }
		await fetchEntities();
	});

	async function fetchEntities() {
		loading = true;
		try {
			const params = new URLSearchParams();
			if (selectedType) params.set('type', selectedType);
			if (selectedBookId) params.set('book_id', selectedBookId);
			if (search.trim()) params.set('search', search.trim());
			params.set('limit', String(limit + 1));
			params.set('offset', String(offset));

			const result = await api.get<Entity[]>(`/entities?${params.toString()}`);
			hasMore = result.length > limit;
			entities = result.slice(0, limit);
		} catch (err) {
			entities = [];
		} finally {
			loading = false;
		}
	}

	function applyFilters() {
		offset = 0;
		fetchEntities();
	}

	function nextPage() {
		offset += limit;
		fetchEntities();
	}

	function prevPage() {
		offset = Math.max(0, offset - limit);
		fetchEntities();
	}

	function handleSearch(e: KeyboardEvent) {
		if (e.key === 'Enter') applyFilters();
	}
</script>

<svelte:head>
	<title>Nova Reader — 实体档案</title>
</svelte:head>

<div class="mx-auto max-w-6xl px-6 py-6 space-y-6">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div class="flex items-center gap-3">
			<Network size={20} class="text-accent-400" />
			<h1 class="text-xl font-semibold text-ink-100">实体档案</h1>
			<span class="text-sm text-ink-500">角色、地点、组织等</span>
		</div>
	</div>

	<!-- Filters -->
	<div class="flex flex-wrap items-center gap-3">
		<!-- Search -->
		<div class="relative flex-1 min-w-[200px] max-w-sm">
			<Search size={14} class="absolute left-3 top-1/2 -translate-y-1/2 text-ink-500" />
			<input
				bind:value={search}
				onkeydown={handleSearch}
				placeholder="搜索实体名称..."
				class="w-full rounded-lg border border-ink-700/50 bg-ink-900/30 py-2 pl-9 pr-3 text-sm text-ink-200 placeholder:text-ink-600 focus:border-accent-500/50 focus:outline-none"
			/>
		</div>

		<!-- Type filter -->
		<select
			bind:value={selectedType}
			onchange={applyFilters}
			class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-xs text-ink-300 focus:border-accent-500/50 focus:outline-none"
		>
			<option value={null}>所有类型</option>
			{#each entityTypes as t}
				<option value={t}>{t}</option>
			{/each}
		</select>

		<!-- Book filter -->
		<select
			bind:value={selectedBookId}
			onchange={applyFilters}
			class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-2 text-xs text-ink-300 focus:border-accent-500/50 focus:outline-none"
		>
			<option value={null}>全部书籍</option>
			{#each books as book}
				<option value={book.id}>{book.title}</option>
			{/each}
		</select>

		<button
			onclick={applyFilters}
			class="rounded-lg bg-accent-500/10 border border-accent-500/20 px-3 py-2 text-xs text-accent-400 hover:bg-accent-500/20 transition-colors"
		>
			<Filter size={12} class="inline mr-1" /> 筛选
		</button>
	</div>

	<!-- Entity Grid -->
	{#if loading}
		<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
			{#each Array(6) as _}
				<div class="h-36 rounded-xl border border-ink-800/50 bg-ink-900/20 animate-pulse"></div>
			{/each}
		</div>
	{:else if entities.length === 0}
		<div class="flex flex-col items-center justify-center py-16 text-center">
			<Users size={48} class="text-ink-700 mb-3" strokeWidth={1} />
			<p class="text-ink-500">未找到实体</p>
			<p class="text-ink-600 text-sm mt-1">尝试调整筛选条件或对书籍运行实体提取</p>
		</div>
	{:else}
		<div class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
			{#each entities as entity (entity.id)}
				<div class="group rounded-xl border border-ink-800/50 bg-ink-900/20 p-4 hover:border-ink-700/70 transition-colors">
					<div class="flex items-start justify-between mb-2">
						<h3 class="text-sm font-medium text-ink-100 group-hover:text-accent-300 transition-colors">{entity.name}</h3>
						<span class="shrink-0 ml-2 rounded-md border px-1.5 py-0.5 text-[10px] font-medium {typeColors[entity.entity_type] || 'bg-ink-800/50 text-ink-400 border-ink-700/50'}">
							{entity.entity_type}
						</span>
					</div>

					{#if entity.description}
						<p class="text-xs text-ink-400 line-clamp-2 mb-2">{entity.description}</p>
					{/if}

					{#if entity.aliases.length > 0}
						<div class="flex flex-wrap gap-1 mb-2">
							{#each entity.aliases.slice(0, 3) as alias}
								<span class="rounded bg-ink-800/50 px-1.5 py-0.5 text-[10px] text-ink-500">{alias}</span>
							{/each}
							{#if entity.aliases.length > 3}
								<span class="text-[10px] text-ink-600">+{entity.aliases.length - 3}</span>
							{/if}
						</div>
					{/if}

					<div class="flex items-center gap-3 mt-auto pt-2 border-t border-ink-800/30 text-[10px] text-ink-500">
						<span>提及 {entity.mention_count} 次</span>
						<span>重要度 {(entity.importance_score * 100).toFixed(0)}%</span>
					</div>
				</div>
			{/each}
		</div>

		<!-- Pagination -->
		<div class="flex items-center justify-between pt-2">
			<span class="text-xs text-ink-500">
				显示 {offset + 1}–{offset + entities.length} 项
			</span>
			<div class="flex gap-2">
				<button
					onclick={prevPage}
					disabled={offset === 0}
					class="rounded-lg border border-ink-700/50 bg-ink-900/30 px-3 py-1.5 text-xs text-ink-400 hover:bg-ink-800/50 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
				>
					<ChevronLeft size={12} class="inline" /> 上一页
				</button>
				<button
					onclick={nextPage}
					disabled={!hasMore}
					class="rounded-lg border border-ink-700/50 bg-ink-900/30 px-3 py-1.5 text-xs text-ink-400 hover:bg-ink-800/50 disabled:opacity-30 disabled:cursor-not-allowed transition-colors"
				>
					下一页 <ChevronRight size={12} class="inline" />
				</button>
			</div>
		</div>
	{/if}
</div>
