<script lang="ts">
	import { api } from '$lib/services/api';
	import { page } from '$app/stores';
	import { Search, Plus, Users, BookOpen, Filter } from 'lucide-svelte';

	import { goto } from '$app/navigation';
	import { onMount } from 'svelte';

	// Initialize from URL params
	let searchQuery = $state($page.url.searchParams.get('q') || '');
	let filterType = $state<string | null>($page.url.searchParams.get('type'));
	let filterBookId = $state<string | null>($page.url.searchParams.get('book_id'));
	let filterSeriesId = $state<string | null>($page.url.searchParams.get('series_id'));
	let selectedCharacter = $state<string | null>($page.url.searchParams.get('selected'));
	let loading = $state(true);
	let showFilters = $state(false);

	let characters = $state.raw<Array<{
		id: string;
		name: string;
		aliases: string[];
		type: string;
		description: string;
		first_appearance: { book: string; chapter: string };
		mention_count: number;
		relationships: Array<{ target: string; type: string }>;
		tags: string[];
		avatar_color: string;
	}>>([]);

	let series = $state.raw<Array<{ id: string; name: string }>>([]);
	let books = $state.raw<Array<{ id: string; title: string }>>([]);

	// Load books and series for filter dropdowns
	onMount(async () => {
		try {
			const [booksResult, seriesResult] = await Promise.all([
				api.getBooks({ per_page: 100 }).catch(() => ({ data: [] })),
				api.getSeriesList?.().catch(() => []),
			]);
			books = (booksResult.data ?? []).map((b) => ({ id: b.id, title: b.title }));
			series = (seriesResult ?? []).map((s) => ({ id: s.id, name: s.name }));
		} catch { /* graceful fallback */ }
	});

	// Bidirectional URL sync
	$effect(() => {
		const params = new URLSearchParams();
		if (searchQuery) params.set('q', searchQuery);
		if (filterType) params.set('type', filterType);
		if (filterBookId) params.set('book_id', filterBookId);
		if (filterSeriesId) params.set('series_id', filterSeriesId);
		if (selectedCharacter) params.set('selected', selectedCharacter);
		const qs = params.toString();
		const newUrl = qs ? `/characters?${qs}` : '/characters';
		if (newUrl !== $page.url.pathname + $page.url.search) {
			goto(newUrl, { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	// Load entities from API — track all filter dependencies
	$effect(() => {
		searchQuery; filterBookId; filterSeriesId; filterType;
		loadCharacters();
	});

	async function loadCharacters() {
		loading = true;
		try {
			const params: Record<string, string> = { type: 'character', limit: '100' };
			if (searchQuery) params.search = searchQuery;
			if (filterBookId) params.book_id = filterBookId;
			if (filterSeriesId) params.series_id = filterSeriesId;
			if (filterType) params.type = filterType;

			const entities = await api.getEntities(params);
			characters = entities.map((e) => ({
				id: e.id,
				name: e.name,
				aliases: e.aliases || [],
				type: e.entity_type ?? 'character',
				description: e.description || '',
				first_appearance: { book: '', chapter: '' },
				mention_count: e.mention_count || 0,
				relationships: [] as Array<{ target: string; type: string }>,
				tags: [] as string[],
				avatar_color: generateColor(e.name),
			}));
		} catch (err) {
			console.error('Failed to load characters:', err);
		} finally {
			loading = false;
		}
	}

	function generateColor(name: string): string {
		let hash = 0;
		for (let i = 0; i < name.length; i++) {
			hash = name.charCodeAt(i) + ((hash << 5) - hash);
		}
		const hue = Math.abs(hash) % 360;
		return `hsl(${hue}, 65%, 55%)`;
	}
</script>

<svelte:head>
	<title>Nova Reader — 人物志</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">人物志</h1>
			<p class="mt-0.5 text-sm text-ink-400">AI 自动提取的角色数据库</p>
		</div>
		<div class="flex items-center gap-3">
			<!-- Search -->
			<div class="relative">
				<Search size={16} strokeWidth={2} class="absolute left-3 top-1/2 -translate-y-1/2 text-ink-400" />
				<input
					type="text"
					bind:value={searchQuery}
					placeholder="搜索人物..."
					class="rounded-lg border border-ink-700/50 bg-ink-900/50 pl-9 pr-4 py-2 text-sm text-ink-100 placeholder-ink-500 outline-none focus:border-accent-500/30 w-48"
				/>
			</div>
			<!-- Filter toggle -->
			<button
				onclick={() => showFilters = !showFilters}
				class="inline-flex items-center gap-2 rounded-lg border border-ink-700/50 px-3 py-2 text-sm transition-colors {showFilters ? 'bg-accent-500/10 text-accent-400 border-accent-500/30' : 'text-ink-400 hover:text-ink-200'}"
			>
				<Filter size={16} strokeWidth={2} />
				筛选
			</button>
			<!-- Add character manually -->
			<button class="inline-flex items-center gap-2 rounded-lg bg-accent-500/10 px-4 py-2 text-sm font-medium text-accent-400 hover:bg-accent-500/20 transition-colors">
				<Plus size={16} strokeWidth={2} />
				手动添加
			</button>
		</div>
	</div>

	<!-- Filter bar -->
	{#if showFilters}
		<div class="flex items-center gap-3 rounded-lg border border-ink-800/50 bg-ink-900/30 p-3 animate-fade-in">
			<!-- Book filter -->
			<div class="flex items-center gap-2">
				<BookOpen size={14} class="text-ink-500" />
				<select
					bind:value={filterBookId}
					class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/30"
				>
					<option value={null}>全部书籍</option>
					{#each books as book}
						<option value={book.id}>{book.title}</option>
					{/each}
				</select>
			</div>
			<!-- Series filter -->
			{#if series.length > 0}
				<div class="flex items-center gap-2">
					<select
						bind:value={filterSeriesId}
						class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3 py-1.5 text-sm text-ink-200 outline-none focus:border-accent-500/30"
					>
						<option value={null}>全部系列</option>
						{#each series as s}
							<option value={s.id}>{s.name}</option>
						{/each}
					</select>
				</div>
			{/if}
			<!-- Entity type filter -->
			<div class="flex items-center gap-1 ml-auto">
				{#each [{ value: null, label: '全部' }, { value: 'character', label: '角色' }, { value: 'location', label: '地点' }, { value: 'item', label: '物品' }, { value: 'organization', label: '组织' }] as typeOpt}
					<button
						onclick={() => filterType = typeOpt.value}
						class="rounded-full px-2.5 py-1 text-xs transition-colors {filterType === typeOpt.value ? 'bg-accent-500/15 text-accent-400' : 'text-ink-400 hover:text-ink-200'}"
					>
						{typeOpt.label}
					</button>
				{/each}
			</div>
			<!-- Clear filters -->
			{#if filterBookId || filterSeriesId || filterType}
				<button
					onclick={() => { filterBookId = null; filterSeriesId = null; filterType = null; }}
					class="text-xs text-ink-500 hover:text-accent-400 transition-colors"
				>
					清除
				</button>
			{/if}
		</div>
	{/if}

	<!-- Character Grid -->
	{#if characters.length === 0}
		<div class="flex flex-col items-center justify-center py-20 text-center">
			<div class="mb-6 flex h-24 w-24 items-center justify-center rounded-3xl bg-ink-800/30 ring-1 ring-ink-700/30">
				<Users size={40} strokeWidth={1} class="text-ink-600" />
			</div>
			<h3 class="text-xl font-semibold text-ink-200">还没有角色数据</h3>
			<p class="mt-2 max-w-md text-sm text-ink-400">
				导入书籍后运行实体提取，AI 将自动识别人物、地点、物品等实体
			</p>
		</div>
	{:else}
		<div class="grid gap-4 sm:grid-cols-2 lg:grid-cols-3 xl:grid-cols-4">
			{#each characters as char}
				<a
					href="/characters/{char.id}"
					class="flex flex-col rounded-xl border border-ink-800/50 bg-ink-900/30 p-4 text-left transition-all hover:border-ink-700/50 hover:bg-ink-800/30 hover:-translate-y-0.5 hover:shadow-lg"
				>
					<!-- Avatar -->
					<div class="mb-3 flex items-center gap-3">
						<div
							class="flex h-12 w-12 items-center justify-center rounded-full text-lg font-bold"
							style="background: {char.avatar_color}20; color: {char.avatar_color}"
						>
							{char.name.charAt(0)}
						</div>
						<div class="flex-1 overflow-hidden">
							<h4 class="truncate font-semibold text-ink-100">{char.name}</h4>
							{#if char.aliases.length > 0}
								<p class="truncate text-xs text-ink-500">
									别名: {char.aliases.join(', ')}
								</p>
							{/if}
						</div>
					</div>

					<!-- Description -->
					<p class="text-xs text-ink-400 line-clamp-2 flex-1">{char.description}</p>

					<!-- Mini relationship graph -->
					{#if char.relationships.length > 0}
						<div class="mt-2 flex items-center gap-1.5">
							<svg width="60" height="28" viewBox="0 0 60 28" class="shrink-0">
								<!-- Center node -->
								<circle cx="30" cy="14" r="4" fill={char.avatar_color} opacity="0.8" />
								<!-- Relationship nodes (max 4) -->
								{#each char.relationships.slice(0, 4) as rel, i}
									{@const angle = (i / Math.min(char.relationships.length, 4)) * Math.PI * 2 - Math.PI / 2}
									{@const rx = 30 + Math.cos(angle) * 18}
									{@const ry = 14 + Math.sin(angle) * 10}
									<line x1="30" y1="14" x2={rx} y2={ry} stroke="currentColor" stroke-width="0.5" class="text-ink-700" />
									<circle cx={rx} cy={ry} r="2.5" class="fill-ink-600" />
								{/each}
							</svg>
							<span class="text-[10px] text-ink-500">{char.relationships.length} 关系</span>
						</div>
					{/if}

					<!-- Stats & Tags -->
					<div class="mt-3 flex flex-wrap gap-1.5">
						{#each char.tags.slice(0, 3) as tag}
							<span class="rounded-md bg-ink-800/80 px-1.5 py-0.5 text-[10px] text-ink-400">{tag}</span>
						{/each}
					</div>

					<div class="mt-3 flex items-center gap-3 border-t border-ink-800/50 pt-3 text-[10px] text-ink-500">
						<span>出现 {char.mention_count} 次</span>
						<span>{char.type === 'character' ? '角色' : char.type === 'location' ? '地点' : char.type}</span>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
