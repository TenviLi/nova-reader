<script lang="ts">
	import { api } from '$services/api';
	import { getErrorMessage } from '$lib/utils';
	import type { CrossBookSearchGroup, SearchResult } from '$types/models';
	import HighlightedText from '$components/HighlightedText.svelte';
	import { Search, X, Clock, Sparkles, ChevronDown, ChevronRight, Filter, BookOpen, Zap, Brain, GitBranch, MoreHorizontal, ExternalLink, Layers, ArrowRight, RefreshCw, Hash } from 'lucide-svelte';
	import { browser } from '$app/environment';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { debounce } from 'es-toolkit';

	type SearchViewMode = 'hybrid' | 'keyword' | 'semantic' | 'graph' | 'global' | 'cross-book';
	const validSearchModes: SearchViewMode[] = ['hybrid', 'keyword', 'semantic', 'graph', 'global', 'cross-book'];

	function parseSearchMode(value: string | null): SearchViewMode {
		return validSearchModes.includes(value as SearchViewMode) ? value as SearchViewMode : 'hybrid';
	}

	// URL-synced state
	let query = $state($page.url.searchParams.get('q') || '');
	let searchMode = $state<SearchViewMode>(parseSearchMode($page.url.searchParams.get('mode')));
	let results = $state.raw<Array<{
		id: string;
		book_id?: string;
		book_title: string;
		chapter_title: string;
		chapter_index: number;
		chunk_index?: number;
		chunk_id?: string;
		content: string;
		content_snippet?: string;
		highlighted?: string;
		score: number;
		rerank_score?: number;
		rerank_explanation?: string;
		rerank_matched_terms?: string[];
		fusion_score?: number;
		keyword_score?: number;
		semantic_score?: number;
		breadcrumb?: string;
		highlight_ranges: Array<[number, number]>;
		entities: string[];
		source?: string;
		match_sources?: string[];
	}>>([]);
		let loading = $state(false);
		let searchError = $state('');
	let totalResults = $state(0);
	let searchTime = $state(0);
	let timing = $state<{ total_ms: number; bm25_ms: number; vector_ms: number; rerank_ms: number } | null>(null);
	let searchHistory = $state<string[]>(
		browser ? JSON.parse(localStorage.getItem('nova_search_history') ?? '[]') : []
	);
	let showHistory = $state(false);
	let expandedBooks = $state<Set<string>>(new Set());
	let expandedContexts = $state<Set<number>>(new Set());
	let contextData = $state<Map<number, Array<{ chunk_id: string; chunk_index: number; content: string; is_target: boolean }>>>(new Map());
	let showTimingDetail = $state(false);
	let similarResults = $state<Map<number, SearchResult[]>>(new Map());
	let loadingSimilar = $state<Set<number>>(new Set());
	let loadingContext = $state<Set<number>>(new Set());

	// Graph search state
	let graphPaths = $state<Array<{ source: string; target: string; nodes: string[]; relationships: string[]; length?: number; path_score?: number; rank_reason?: string; explanation?: string }>>([]);
	let graphEntities = $state<string[]>([]);
	let graphRelatedEntities = $state<string[]>([]);

	// Retrieval explainability
	let searchIntent = $state<string | null>(null);
	let rerankApplied = $state(false);

	// Global search state
	let globalAnswer = $state('');
	let globalSources = $state<Array<{ community_id: string; summary: string; members: string[] }>>([]);
	let globalCommunitiesAnalyzed = $state(0);

	// Cross-book comparison state
	let crossBookGroups = $state.raw<CrossBookSearchGroup[]>([]);

	// Typeahead suggestions
	let suggestions = $state<Array<{text: string; type: string}>>([]);
	let showSuggestions = $state(false);

	// Facets
	let facets = $state<{ format: Array<{value: string; count: number}>; author: Array<{value: string; count: number}> }>({ format: [], author: [] });
	let showFacets = $state(false);

	// Idle state (no search in progress, no results yet) — center layout like Google
	let isIdle = $derived(!loading && results.length === 0 && !globalAnswer && crossBookGroups.length === 0);

	// Group results by book_title
	let groupedResults = $derived(() => {
		const groups = new Map<string, typeof results>();
		for (const r of results) {
			const key = r.book_title || 'unknown';
			if (!groups.has(key)) groups.set(key, []);
			groups.get(key)!.push(r);
		}
		return groups;
	});

	// Normalize scores for display: map to 0-100 relative to max score in result set
	let maxScore = $derived(results.length > 0 ? Math.max(...results.map(r => r.rerank_score ?? r.score)) : 1);
	function normalizeScore(score: number): number {
		if (maxScore <= 0) return 0;
		// Rank-relative normalization: top result = 95, scale linearly
		return Math.max(5, Math.round((score / maxScore) * 95));
	}

	// Build a short human-readable explanation of why a result ranked where it did.
	function matchReason(r: typeof results[number]): string {
		const parts: string[] = [];
		const srcs = r.match_sources ?? (r.source ? [r.source] : []);
		const labelMap: Record<string, string> = {
			keyword: '关键词', semantic: '语义', graph: '图谱', database: '书库',
		};
		if (srcs.length > 1) {
			parts.push(`${srcs.map((s) => labelMap[s] ?? s).join(' + ')} 多路命中`);
		} else if (srcs.length === 1) {
			parts.push(`${labelMap[srcs[0]] ?? srcs[0]} 命中`);
		}
		if (r.rerank_score != null) {
			const pct = Math.round(r.rerank_score * 100);
			const tier = r.rerank_score >= 0.66 ? '高度相关' : r.rerank_score >= 0.33 ? '中等相关' : '弱相关';
			parts.push(`重排${tier} (${pct}%)`);
		}
		return parts.join(' · ');
	}

	// Detailed per-source score tooltip for a result.
	function scoreBreakdown(r: typeof results[number]): string {
		const lines: string[] = [];
		if (r.keyword_score != null) lines.push(`关键词原始分: ${r.keyword_score.toFixed(3)}`);
		if (r.semantic_score != null) lines.push(`语义相似度: ${r.semantic_score.toFixed(3)}`);
		if (r.fusion_score != null) lines.push(`RRF 融合分: ${r.fusion_score.toFixed(4)}`);
		if (r.rerank_score != null) lines.push(`重排相关度: ${r.rerank_score.toFixed(3)}`);
		if (lines.length === 0) lines.push(`分数: ${r.score.toFixed(3)}`);
		return lines.join('\n');
	}

	function toggleBookGroup(bookTitle: string) {
		const next = new Set(expandedBooks);
		if (next.has(bookTitle)) next.delete(bookTitle);
		else next.add(bookTitle);
		expandedBooks = next;
	}

	const debouncedSearch = debounce(() => {
		if (query.trim().length >= 2) performSearch();
	}, 350);

	const debouncedSuggest = debounce(async () => {
		if (query.trim().length < 2) {
			suggestions = [];
			return;
		}
		try {
			const resp = await api.searchSuggest(query.trim());
			suggestions = resp.suggestions || [];
			showSuggestions = suggestions.length > 0;
		} catch {
			suggestions = [];
		}
	}, 200);

	// Auto-search from URL params on mount
	let initialSearchDone = $state(false);
	$effect(() => {
		if (browser && query && !initialSearchDone && !loading) {
			initialSearchDone = true;
			performSearch();
		}
	});

	const searchModes = [
		{ value: 'hybrid', label: '混合搜索', icon: Layers, description: 'BM25 + 向量 + RRF 融合' },
		{ value: 'keyword', label: '关键词', icon: Hash, description: 'Meilisearch 全文索引' },
		{ value: 'semantic', label: '语义', icon: Brain, description: 'Qwen3 向量相似度' },
		{ value: 'graph', label: '图谱', icon: GitBranch, description: '实体关系遍历 + 路径查询' },
		{ value: 'global', label: '全局分析', icon: Sparkles, description: '社区摘要 → Map-Reduce 综合回答' },
		{ value: 'cross-book', label: '跨书对比', icon: BookOpen, description: '按书分组比较同一主题的命中' },
	] as const;

	const discoveryPrompts: Array<{
		label: string;
		description: string;
		query: string;
		mode: SearchViewMode;
		icon: typeof Search;
	}> = [
		{ label: '主题分类', description: '按设定、流派、世界观寻找内容', query: '修炼体系 世界设定 门派势力', mode: 'semantic', icon: Layers },
		{ label: '情绪浏览', description: '寻找轻松、热血、悬疑或治愈片段', query: '轻松幽默 日常 治愈 热血高潮', mode: 'hybrid', icon: Sparkles },
		{ label: '人物关系', description: '从角色与组织关系进入图谱搜索', query: '主角 师徒 敌对 盟友', mode: 'graph', icon: GitBranch },
		{ label: '相似段落', description: '输入一段氛围文本寻找近似片段', query: '夜色中独自修炼 突破境界 命运转折', mode: 'semantic', icon: Brain },
		{ label: '跨书对比', description: '比较同一设定在不同书里的写法', query: '师徒传承 资源争夺 境界突破', mode: 'cross-book', icon: BookOpen },
	];

	function runDiscoveryPrompt(prompt: (typeof discoveryPrompts)[number]) {
		query = prompt.query;
		searchMode = prompt.mode;
		performSearch();
	}

	function normalizeCrossBookResults(groups: CrossBookSearchGroup[]): typeof results {
		return groups.flatMap((group) =>
			group.chunks.map((chunk, index) => ({
				id: chunk.chunk_id ?? chunk.id ?? `${group.book_id}-${chunk.chapter_index}-${chunk.chunk_index ?? index}`,
				book_id: group.book_id,
				book_title: group.book_title,
				chapter_title: chunk.chapter_title ?? '',
				chapter_index: chunk.chapter_index ?? 0,
				chunk_index: chunk.chunk_index ?? undefined,
				chunk_id: chunk.chunk_id ?? chunk.id,
				content: chunk.content,
				content_snippet: chunk.content,
				highlighted: chunk.highlighted ?? undefined,
				score: chunk.score ?? group.top_score ?? 0,
				highlight_ranges: [],
				entities: [],
				source: 'database',
				match_sources: ['book'],
			}))
		);
	}

	async function performSearch() {
		if (!query.trim()) return;
			loading = true;
			searchError = '';
		showHistory = false;
		graphPaths = [];
		graphEntities = [];
		graphRelatedEntities = [];
		searchIntent = null;
		rerankApplied = false;
		globalAnswer = '';
		globalSources = [];
		globalCommunitiesAnalyzed = 0;
		crossBookGroups = [];

		const params = new URLSearchParams();
		params.set('q', query.trim());
		if (searchMode !== 'hybrid') params.set('mode', searchMode);
		goto(`/search?${params.toString()}`, { replaceState: true, keepFocus: true, noScroll: true });

		const startTime = performance.now();
		try {
			if (searchMode === 'global') {
				const resp = await api.searchGlobal(query.trim());
				globalAnswer = resp.answer;
				globalSources = resp.sources || [];
				globalCommunitiesAnalyzed = resp.communities_analyzed || 0;
				results = [];
				totalResults = 0;
				timing = null;
				searchTime = resp.timing?.total_ms ?? Math.round(performance.now() - startTime);
			} else if (searchMode === 'cross-book') {
				const resp = await api.searchCrossBook(query.trim(), undefined, 36);
				crossBookGroups = resp.groups || [];
				results = normalizeCrossBookResults(crossBookGroups);
				totalResults = resp.total ?? results.length;
				timing = null;
				searchTime = resp.timing?.total_ms ?? Math.round(performance.now() - startTime);
			} else if (searchMode === 'graph') {
				const [graphResp, facetResp] = await Promise.all([
					api.searchGraph(query.trim()),
					api.searchFacets(query.trim()).catch(() => ({ facets: { format: [], author: [] } })),
				]);
				results = (graphResp.results || []).map((r) => ({
					...r,
					content: r.content || r.content_snippet || '',
					entities: r.entities ?? [],
				}));
				totalResults = results.length;
				graphPaths = graphResp.paths || [];
				graphEntities = graphResp.entities || [];
				graphRelatedEntities = graphResp.related_entities || [];
				timing = null;
				searchTime = graphResp.timing?.total_ms ?? Math.round(performance.now() - startTime);
				facets = facetResp.facets || { format: [], author: [] };
			} else {
				const [response, facetResp] = await Promise.all([
					api.search({ query: query.trim(), mode: searchMode, limit: 20 }),
					api.searchFacets(query.trim()).catch(() => ({ facets: { format: [], author: [] } })),
				]);
				results = response.results.map((r) => ({
					...r,
					content: r.content || r.content_snippet || '',
					entities: r.entities ?? [],
				}));
				totalResults = response.total;
				timing = response.timing ?? null;
				searchIntent = response.intent ?? null;
				rerankApplied = response.rerank_applied ?? false;
				searchTime = timing?.total_ms ?? Math.round(performance.now() - startTime);
				facets = facetResp.facets || { format: [], author: [] };
			}

			if (!searchHistory.includes(query.trim())) {
				searchHistory = [query.trim(), ...searchHistory.slice(0, 19)];
				if (browser) localStorage.setItem('nova_search_history', JSON.stringify(searchHistory));
			}
			} catch (e: unknown) {
				results = [];
				crossBookGroups = [];
				totalResults = 0;
				timing = null;
				searchError = getErrorMessage(e) || '搜索服务暂时不可用';
			} finally {
				loading = false;
			}
	}

	function clearHistory() {
		searchHistory = [];
		if (browser) localStorage.removeItem('nova_search_history');
	}

	function selectHistoryItem(item: string) {
		query = item;
		showHistory = false;
		performSearch();
	}

	let selectedResultIdx = $state(-1);

	function handleKeydown(e: KeyboardEvent) {
		if (e.key === 'Enter') {
			if (selectedResultIdx >= 0 && results[selectedResultIdx]) {
				goto(`/library/${results[selectedResultIdx].book_id || results[selectedResultIdx].id}`);
			} else {
				performSearch();
			}
		} else if (e.key === 'ArrowDown' && results.length > 0) {
			e.preventDefault();
			selectedResultIdx = Math.min(selectedResultIdx + 1, results.length - 1);
			document.getElementById(`search-result-${selectedResultIdx}`)?.scrollIntoView({ block: 'nearest' });
		} else if (e.key === 'ArrowUp' && results.length > 0) {
			e.preventDefault();
			selectedResultIdx = Math.max(selectedResultIdx - 1, -1);
			if (selectedResultIdx >= 0) {
				document.getElementById(`search-result-${selectedResultIdx}`)?.scrollIntoView({ block: 'nearest' });
			}
		}
	}

	$effect(() => { results; selectedResultIdx = -1; });

	function handleInput() {
		debouncedSearch();
		debouncedSuggest();
	}

	function sanitizeHighlight(html: string): string {
		return html
			.replace(/&/g, '&amp;')
			.replace(/</g, '&lt;')
			.replace(/>/g, '&gt;')
			.replace(/&lt;mark&gt;/g, '<mark>')
			.replace(/&lt;\/mark&gt;/g, '</mark>')
			// Merge adjacent <mark> tags (Meilisearch splits CJK into single chars)
			.replace(/<\/mark><mark>/g, '');
	}

	async function loadContext(globalIdx: number) {
		const result = results[globalIdx];
		if (!result?.chunk_id && !result?.id) return;
		const chunkId = result.chunk_id || result.id;

		const nextLoading = new Set(loadingContext);
		nextLoading.add(globalIdx);
		loadingContext = nextLoading;

		try {
			const resp = await api.getChunkContext(chunkId, 2);
			const nextData = new Map(contextData);
			nextData.set(globalIdx, resp.context || []);
			contextData = nextData;
		} catch {
			const nextData = new Map(contextData);
			nextData.set(globalIdx, []);
			contextData = nextData;
		} finally {
			const next = new Set(loadingContext);
			next.delete(globalIdx);
			loadingContext = next;
		}
	}

	function toggleContext(globalIdx: number) {
		const next = new Set(expandedContexts);
		if (next.has(globalIdx)) {
			next.delete(globalIdx);
		} else {
			next.add(globalIdx);
			if (!contextData.has(globalIdx)) loadContext(globalIdx);
		}
		expandedContexts = next;
	}

	async function loadSimilar(globalIdx: number) {
		const result = results[globalIdx];
		const chunkId = result?.chunk_id || result?.id;
		if (!chunkId) return;

		const nextLoading = new Set(loadingSimilar);
		nextLoading.add(globalIdx);
		loadingSimilar = nextLoading;

		try {
			const resp = await api.findSimilar(chunkId, 5);
			const nextData = new Map(similarResults);
			nextData.set(globalIdx, resp);
			similarResults = nextData;
		} catch {
			const nextData = new Map(similarResults);
			nextData.set(globalIdx, []);
			similarResults = nextData;
		} finally {
			const next = new Set(loadingSimilar);
			next.delete(globalIdx);
			loadingSimilar = next;
		}
	}
</script>

<svelte:head>
	<title>Nova Reader — 智能搜索</title>
</svelte:head>

<div class="mx-auto max-w-5xl px-4 sm:px-6 lg:px-8 space-y-6 animate-fade-in {isIdle ? 'min-h-[calc(100dvh-4rem)] flex flex-col justify-center' : 'py-6'}">
	<!-- Header -->
	<div class="text-center">
		<h1 class="text-3xl font-bold text-ink-50 tracking-tight">智能搜索</h1>
		<p class="mt-1.5 text-sm text-ink-400">跨书库全文检索 · Qwen3 语义理解 · 知识图谱推理</p>
	</div>

	<!-- Search Input -->
	<div class="relative">
		<div class="flex items-center gap-3 rounded-2xl border border-ink-700/50 bg-ink-900/50 px-5 py-4 shadow-lg focus-within:border-accent-500/30 focus-within:shadow-glow transition-all">
			<Search size={20} strokeWidth={2} class="shrink-0 text-ink-400" />
			<input
				type="text"
				bind:value={query}
				onkeydown={handleKeydown}
				oninput={handleInput}
				onfocus={() => showHistory = searchHistory.length > 0 && !query}
				onblur={() => setTimeout(() => showHistory = false, 200)}
				placeholder="搜索任何内容：角色、情节、设定、对话..."
				class="w-full bg-transparent text-lg text-ink-100 placeholder-ink-500 outline-none"
				aria-label="搜索输入"
			/>
			{#if query}
				<button onclick={() => { query = ''; results = []; crossBookGroups = []; globalAnswer = ''; }} class="shrink-0 text-ink-400 hover:text-ink-200 transition-colors">
					<X size={20} strokeWidth={2} />
				</button>
			{/if}
			<button
				onclick={performSearch}
				disabled={loading}
				class="shrink-0 rounded-xl bg-accent-500 px-5 py-2 text-sm font-medium text-ink-950 hover:bg-accent-400 disabled:opacity-50 transition-colors"
			>
				{loading ? '...' : '搜索'}
			</button>
		</div>

		<!-- History dropdown -->
		{#if showHistory && searchHistory.length > 0}
			<div class="absolute left-0 right-0 top-full z-50 mt-2 rounded-xl border border-ink-700/50 bg-ink-900/95 p-2 shadow-xl backdrop-blur-sm">
				<div class="flex items-center justify-between px-3 py-1.5">
					<span class="text-xs font-medium text-ink-400">搜索历史</span>
					<button onclick={clearHistory} class="text-xs text-ink-500 hover:text-ink-300 transition-colors">清除</button>
				</div>
				{#each searchHistory.slice(0, 8) as item}
					<button
						onclick={() => selectHistoryItem(item)}
						class="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm text-ink-300 hover:bg-ink-800/60 hover:text-ink-100 transition-colors"
					>
						<Clock size={14} class="shrink-0 text-ink-500" />
						<span class="truncate">{item}</span>
					</button>
				{/each}
			</div>
		{/if}

		<!-- Typeahead -->
		{#if showSuggestions && suggestions.length > 0 && !showHistory}
			<div class="absolute left-0 right-0 top-full z-50 mt-2 rounded-xl border border-ink-700/50 bg-ink-900/95 p-2 shadow-xl backdrop-blur-sm">
				<span class="px-3 py-1 text-xs font-medium text-ink-400">建议</span>
				{#each suggestions as suggestion}
					<button
						onclick={() => { query = suggestion.text; showSuggestions = false; performSearch(); }}
						class="flex w-full items-center gap-2.5 rounded-lg px-3 py-2 text-left text-sm text-ink-300 hover:bg-ink-800/60 hover:text-ink-100 transition-colors"
					>
						<Sparkles size={14} class="shrink-0 text-accent-400" />
						<span class="truncate">{suggestion.text}</span>
						<span class="ml-auto text-[10px] text-ink-500">{suggestion.type}</span>
					</button>
				{/each}
			</div>
		{/if}
	</div>

	<!-- Mode Tabs -->
	<div class="flex flex-wrap justify-center gap-1.5">
		{#each searchModes as mode}
			{@const Icon = mode.icon}
			<button
				onclick={() => { searchMode = mode.value; if (query.trim()) performSearch(); }}
				class="flex items-center gap-1.5 rounded-lg px-4 py-2 text-sm transition-all {searchMode === mode.value ? 'bg-accent-500/10 border border-accent-500/20 text-accent-400 shadow-sm' : 'text-ink-400 hover:bg-ink-800/50 hover:text-ink-300'}"
				title={mode.description}
				type="button"
				aria-pressed={searchMode === mode.value}
			>
				<Icon size={14} />
				{mode.label}
			</button>
		{/each}
	</div>

	{#if isIdle}
		<div class="grid gap-3 sm:grid-cols-2">
			{#each discoveryPrompts as prompt}
				{@const Icon = prompt.icon}
				<button
					onclick={() => runDiscoveryPrompt(prompt)}
					class="group rounded-xl border border-ink-800/50 bg-ink-900/30 p-4 text-left transition-all hover:-translate-y-0.5 hover:border-accent-500/25 hover:bg-ink-900/60"
					type="button"
				>
					<div class="flex items-center gap-3">
						<div class="flex h-9 w-9 shrink-0 items-center justify-center rounded-lg bg-accent-500/10 text-accent-400 ring-1 ring-accent-500/15">
							<Icon size={17} />
						</div>
						<div class="min-w-0">
							<div class="text-sm font-medium text-ink-100 group-hover:text-accent-300 transition-colors">{prompt.label}</div>
							<div class="mt-0.5 text-xs text-ink-500">{prompt.description}</div>
						</div>
					</div>
				</button>
			{/each}
		</div>
	{/if}

	<!-- Loading skeleton -->
	{#if loading}
		<div class="space-y-3">
			{#each Array(4) as _}
				<div class="animate-pulse rounded-xl border border-ink-800/50 p-5">
					<div class="flex items-center gap-2 mb-3">
						<div class="h-3 w-24 rounded bg-ink-800/50"></div>
						<div class="ml-auto h-4 w-12 rounded-full bg-ink-800/50"></div>
					</div>
					<div class="space-y-2">
						<div class="h-3 w-full rounded bg-ink-800/30"></div>
						<div class="h-3 w-4/5 rounded bg-ink-800/30"></div>
						<div class="h-3 w-3/5 rounded bg-ink-800/30"></div>
					</div>
				</div>
			{/each}
		</div>

	<!-- Global search answer -->
	{:else if searchMode === 'global' && globalAnswer}
		<div class="rounded-xl border border-violet-500/20 bg-violet-500/5 p-5 space-y-4">
			<div class="flex items-center justify-between">
				<h3 class="text-sm font-medium text-violet-400 flex items-center gap-1.5">
					<Sparkles size={14} />
					全局分析结果
				</h3>
				<span class="text-[11px] text-ink-500">分析了 {globalCommunitiesAnalyzed} 个社区</span>
			</div>
			<p class="text-sm text-ink-200 leading-relaxed whitespace-pre-wrap">{globalAnswer}</p>
			{#if globalSources.length > 0}
				<div class="space-y-2 pt-2 border-t border-violet-500/10">
					<span class="text-xs text-ink-500">参考社区:</span>
					{#each globalSources as source}
						<div class="rounded-lg bg-ink-900/50 p-3 space-y-1">
							<div class="flex items-center gap-2">
								<span class="text-xs font-mono text-violet-400">{source.community_id}</span>
								<div class="flex flex-wrap gap-1">
									{#each source.members.slice(0, 5) as member}
										<button onclick={() => { query = member; searchMode = 'semantic'; performSearch(); }} class="rounded bg-violet-500/10 px-1.5 py-0.5 text-[10px] text-violet-300 hover:bg-violet-500/20">{member}</button>
									{/each}
								</div>
							</div>
							<p class="text-xs text-ink-400">{source.summary}</p>
						</div>
					{/each}
				</div>
			{/if}
		</div>

	<!-- Results -->
	{:else if results.length > 0}
		<!-- Meta bar -->
		<div class="flex items-center justify-between text-sm text-ink-400">
			<span class="flex items-center gap-2">
				找到 <strong class="text-ink-200">{totalResults}</strong> 条结果
				{#if searchIntent && searchMode !== 'graph'}
					<span class="inline-flex items-center gap-1 rounded-full bg-accent-500/10 px-2 py-0.5 text-[10px] text-accent-300" title="根据查询自动识别的检索意图，用于指导重排器">
						<Brain size={10} />
						意图：{searchIntent}
					</span>
				{/if}
				{#if rerankApplied}
					<span class="inline-flex items-center gap-1 rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-300" title="已用 Qwen3-Reranker 对候选结果重新排序">
						<Sparkles size={10} />
						已重排
					</span>
				{/if}
			</span>
			<div class="flex items-center gap-3">
				{#if facets.format.length > 0 || facets.author.length > 0}
					<button onclick={() => showFacets = !showFacets} class="flex items-center gap-1 text-ink-400 hover:text-ink-200 transition-colors">
						<Filter size={14} />
						<span>筛选</span>
					</button>
				{/if}
				<button
					onclick={() => showTimingDetail = !showTimingDetail}
					class="flex items-center gap-1.5 text-ink-500 hover:text-ink-300 transition-colors tabular-nums"
					title="详细耗时"
				>
					<Zap size={12} />
					<span>{searchTime}ms</span>
				</button>
			</div>
		</div>

		<!-- Timing breakdown -->
		{#if showTimingDetail && timing}
			<div class="rounded-lg border border-ink-800/50 bg-ink-900/40 px-4 py-2.5 flex items-center gap-4 text-xs tabular-nums">
				<span class="text-ink-400">BM25: <span class="text-blue-400">{timing.bm25_ms}ms</span></span>
				<span class="text-ink-400">向量: <span class="text-violet-400">{timing.vector_ms}ms</span></span>
				<span class="text-ink-400">重排: <span class="text-amber-400">{timing.rerank_ms}ms</span></span>
				<span class="text-ink-400">总计: <span class="text-ink-200">{timing.total_ms}ms</span></span>
				<div class="flex-1 flex h-1.5 rounded-full overflow-hidden bg-ink-800/50 ml-2">
					<div class="bg-blue-500/80 h-full" style="width: {(timing.bm25_ms / (timing.total_ms || 1)) * 100}%"></div>
					<div class="bg-violet-500/80 h-full" style="width: {(timing.vector_ms / (timing.total_ms || 1)) * 100}%"></div>
					<div class="bg-amber-500/80 h-full" style="width: {(timing.rerank_ms / (timing.total_ms || 1)) * 100}%"></div>
				</div>
			</div>
		{/if}

		<!-- Graph paths (graph mode only) -->
		{#if searchMode === 'graph' && graphPaths.length > 0}
			<div class="rounded-xl border border-emerald-500/20 bg-emerald-500/5 p-4 space-y-3">
				<h3 class="text-sm font-medium text-emerald-400 flex items-center gap-1.5">
					<GitBranch size={14} />
					关系路径
					<span class="text-[10px] font-normal text-emerald-500/60">按路径相关度排序</span>
				</h3>
				{#each graphPaths as path, pi}
					<div class="flex items-center gap-2 flex-wrap text-sm">
						<span class="rounded bg-emerald-500/15 px-1.5 py-0.5 text-[10px] font-medium text-emerald-300" title={path.rank_reason || '路径相关度'}>
							{Math.round((path.path_score ?? 0) * 100)} · {path.length ?? Math.max(0, path.nodes.length - 1)} 跳{pi === 0 ? ' · 最强' : ''}
						</span>
						{#each path.nodes as node, ni}
							<button onclick={() => { query = node; searchMode = 'semantic'; performSearch(); }} class="rounded-md bg-emerald-500/10 px-2 py-0.5 text-emerald-300 font-medium hover:bg-emerald-500/20 transition-colors">{node}</button>
							{#if ni < path.nodes.length - 1}
								<span class="flex items-center gap-0.5 text-ink-500">
									<ArrowRight size={12} />
									<span class="text-[10px] text-emerald-500/70">{path.relationships[ni] || '—'}</span>
								</span>
							{/if}
						{/each}
						{#if path.rank_reason}
							<span class="text-[10px] text-emerald-500/70">{path.rank_reason}</span>
						{/if}
					</div>
				{/each}
			</div>
		{/if}

		<!-- Graph related entities -->
		{#if searchMode === 'graph' && graphRelatedEntities.length > 0}
			<div class="flex flex-wrap items-center gap-1.5 px-1">
				<span class="text-xs text-ink-500">相关实体:</span>
				{#each graphRelatedEntities.slice(0, 12) as entity}
					<button
						onclick={() => { query = entity; performSearch(); }}
						class="rounded-md bg-ink-800/50 px-2 py-0.5 text-xs text-ink-300 hover:bg-accent-500/10 hover:text-accent-400 transition-colors"
					>
						{entity}
					</button>
				{/each}
			</div>
		{/if}

		{#if searchMode === 'cross-book' && crossBookGroups.length > 0}
			<div class="rounded-xl border border-sky-500/20 bg-sky-500/5 p-4">
				<div class="flex flex-col gap-1 sm:flex-row sm:items-center sm:justify-between">
					<h3 class="flex items-center gap-1.5 text-sm font-medium text-sky-300">
						<BookOpen size={14} />
						跨书对比
					</h3>
					<span class="text-xs text-ink-500">覆盖 {crossBookGroups.length} 本书 · {totalResults} 个片段</span>
				</div>
				<div class="mt-3 grid gap-2 sm:grid-cols-2">
					{#each crossBookGroups.slice(0, 6) as group}
						<a href="/library/{group.book_id}" class="min-w-0 rounded-lg border border-ink-800/50 bg-ink-950/30 px-3 py-2 transition-colors hover:border-sky-500/30 hover:bg-sky-500/5">
							<div class="flex items-center gap-2">
								<span class="truncate text-sm font-medium text-ink-100">《{group.book_title || '未命名书籍'}》</span>
								<span class="ml-auto shrink-0 text-[11px] text-ink-500">{group.count} 条</span>
							</div>
							<div class="mt-2 flex items-center gap-2">
								<div class="h-1.5 flex-1 overflow-hidden rounded-full bg-ink-800">
									<div class="h-full rounded-full bg-sky-400" style="width: {normalizeScore(group.top_score)}%"></div>
								</div>
								<span class="w-8 text-right text-[10px] tabular-nums text-sky-300">{normalizeScore(group.top_score)}</span>
							</div>
						</a>
					{/each}
				</div>
			</div>
		{/if}

		<!-- Facets -->
		{#if showFacets && (facets.format.length > 0 || facets.author.length > 0)}
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4 space-y-3">
				{#if facets.format.length > 0}
					<div>
						<span class="text-xs font-medium text-ink-400 uppercase tracking-wide">格式</span>
						<div class="mt-1.5 flex flex-wrap gap-1.5">
							{#each facets.format as f}
								<span class="rounded-md bg-ink-800/50 px-2 py-0.5 text-xs text-ink-300">{f.value} ({f.count})</span>
							{/each}
						</div>
					</div>
				{/if}
				{#if facets.author.length > 0}
					<div>
						<span class="text-xs font-medium text-ink-400 uppercase tracking-wide">作者</span>
						<div class="mt-1.5 flex flex-wrap gap-1.5">
							{#each facets.author as a}
								<span class="rounded-md bg-ink-800/50 px-2 py-0.5 text-xs text-ink-300">{a.value} ({a.count})</span>
							{/each}
						</div>
					</div>
				{/if}
			</div>		{/if}

		<!-- Results grouped by book -->
		<div class="space-y-4">
			{#each [...groupedResults().entries()] as [bookTitle, bookResults]}
				{@const showAll = expandedBooks.has(bookTitle)}
				{@const visibleResults = showAll ? bookResults : bookResults.slice(0, 3)}
				{@const firstResult = bookResults[0]}

				<!-- Book group card -->
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/40 overflow-hidden">
					<!-- Book header -->
					<div class="flex items-center gap-3 px-5 py-3 border-b border-ink-800/30 bg-ink-900/60">
						<BookOpen size={16} class="shrink-0 text-accent-500/70" />
						<a href="/library/{firstResult.book_id || firstResult.id}" class="text-sm font-medium text-ink-100 hover:text-accent-400 transition-colors truncate">
							《{bookTitle}》
						</a>
						<span class="ml-auto text-[11px] text-ink-500 tabular-nums shrink-0">{bookResults.length} 条匹配</span>
					</div>

					<!-- Results inside this book -->
					<div class="divide-y divide-ink-800/20">
						{#each visibleResults as result, ri}
						{@const globalIdx = results.indexOf(result)}
						{@const isExpanded = expandedContexts.has(globalIdx)}
						{@const displayScore = result.rerank_score ?? result.score}
						{@const reason = matchReason(result)}
				<div
					id="search-result-{globalIdx}"
					class="group transition-all hover:bg-ink-800/20 {selectedResultIdx === globalIdx ? 'bg-accent-500/5' : ''}"
				>
					<!-- Header -->
					<div class="flex items-center gap-2 px-5 pt-3 pb-2">
						<span class="flex items-center gap-1.5 text-sm text-ink-300 truncate min-w-0">
							<Hash size={12} class="shrink-0 text-ink-600" />
							<span class="text-ink-300 truncate">{result.chapter_title || `第${(result.chapter_index || 0) + 1}章`}</span>
						</span>
						<div class="ml-auto flex items-center gap-2 shrink-0">
							{#if result.source}
								{@const sourceConfig = {
									keyword: { label: '关键词', color: 'bg-blue-500/10 text-blue-400 border-blue-500/20' },
									semantic: { label: '语义', color: 'bg-violet-500/10 text-violet-400 border-violet-500/20' },
									hybrid: { label: '混合', color: 'bg-accent-500/10 text-accent-300 border-accent-500/20' },
									graph: { label: '图谱', color: 'bg-emerald-500/10 text-emerald-400 border-emerald-500/20' },
									database: { label: '书库', color: 'bg-orange-500/10 text-orange-400 border-orange-500/20' },
								}[result.source] ?? { label: result.source, color: 'bg-ink-800/50 text-ink-400 border-ink-700/50' }}
								{@const sourceTitle = result.match_sources?.length ? `来源：${result.match_sources.join(' + ')}` : `来源：${sourceConfig.label}`}
								<span class="inline-flex items-center rounded-full border px-2 py-0.5 text-[10px] font-medium {sourceConfig.color}" title={sourceTitle}>{sourceConfig.label}</span>
							{/if}
							<div class="flex items-center gap-1.5" title={scoreBreakdown(result)}>
								<div class="h-1.5 w-12 rounded-full bg-ink-800">
									<div class="h-full rounded-full {normalizeScore(displayScore) > 70 ? 'bg-emerald-500' : normalizeScore(displayScore) > 40 ? 'bg-accent-500' : 'bg-amber-500'}" style="width: {normalizeScore(displayScore)}%"></div>
								</div>
								<span class="text-[10px] tabular-nums text-ink-500">{normalizeScore(displayScore)}</span>
							</div>
						</div>
					</div>

					<!-- Match reason + per-source score breakdown -->
					{#if reason || result.keyword_score != null || result.semantic_score != null}
						<div class="flex flex-wrap items-center gap-x-3 gap-y-1 px-5 pb-1 text-[10px] text-ink-500">
							{#if reason}
								<span class="inline-flex items-center gap-1 text-ink-400">
									<Sparkles size={10} class="text-accent-400/70" />
									{reason}
								</span>
							{/if}
							{#if result.keyword_score != null}
								<span class="inline-flex items-center gap-1" title="关键词检索（BM25）原始分">
									<span class="h-1 w-8 rounded-full bg-ink-800 overflow-hidden"><span class="block h-full bg-blue-500/70" style="width: {Math.min(100, Math.round(result.keyword_score * 100))}%"></span></span>
									<span class="text-blue-400/80">词 {result.keyword_score.toFixed(2)}</span>
								</span>
							{/if}
							{#if result.semantic_score != null}
								<span class="inline-flex items-center gap-1" title="向量语义相似度（余弦）">
									<span class="h-1 w-8 rounded-full bg-ink-800 overflow-hidden"><span class="block h-full bg-violet-500/70" style="width: {Math.min(100, Math.round(result.semantic_score * 100))}%"></span></span>
									<span class="text-violet-400/80">义 {result.semantic_score.toFixed(2)}</span>
								</span>
							{/if}
							{#if result.rerank_score != null}
								<span class="inline-flex items-center gap-1" title="Qwen3 重排相关度">
									<span class="h-1 w-8 rounded-full bg-ink-800 overflow-hidden"><span class="block h-full bg-amber-500/70" style="width: {Math.min(100, Math.round(result.rerank_score * 100))}%"></span></span>
									<span class="text-amber-400/80">排 {result.rerank_score.toFixed(2)}</span>
								</span>
							{/if}
							{#if result.rerank_explanation}
								<span class="basis-full inline-flex min-w-0 items-center gap-1 text-ink-400" title={result.rerank_matched_terms?.length ? `命中词：${result.rerank_matched_terms.join('、')}` : '重排命中句'}>
									<Sparkles size={10} class="shrink-0 text-amber-400/70" />
									<span class="truncate">{result.rerank_explanation}</span>
								</span>
							{/if}
						</div>
					{/if}

					<!-- Content -->
					<div class="px-5 pb-3">
						{#if result.highlighted}
							<p class="text-sm text-ink-300 leading-relaxed line-clamp-4 search-highlight">{@html sanitizeHighlight(result.highlighted)}</p>
						{:else}
							<p class="text-sm text-ink-300 leading-relaxed line-clamp-4"><HighlightedText text={result.content} query={query} /></p>
						{/if}
					</div>

					<!-- Footer -->
					<div class="flex items-center gap-2 px-5 pb-3 pt-1 border-t border-ink-800/20">
						{#if result.entities.length > 0}
							<div class="flex flex-wrap gap-1 min-w-0">
								{#each result.entities.slice(0, 5) as entity}
									<a href="/search?q={encodeURIComponent(entity)}&mode=semantic" class="rounded-md bg-ink-800/60 px-1.5 py-0.5 text-[10px] text-ink-300 hover:bg-accent-500/10 hover:text-accent-300 transition-colors truncate max-w-[80px]" title={entity}>{entity}</a>
								{/each}
								{#if result.entities.length > 5}
									<span class="text-[10px] text-ink-500">+{result.entities.length - 5}</span>
								{/if}
							</div>
						{/if}
						<div class="ml-auto flex items-center gap-0.5">
							{#if result.chunk_id}
							<button onclick={() => toggleContext(globalIdx)} class="rounded-md px-2 py-1 text-[11px] text-ink-500 hover:text-ink-200 hover:bg-ink-800/50 transition-colors flex items-center gap-1" title="展开上下文">
								<MoreHorizontal size={12} />
								<span class="hidden sm:inline">上下文</span>
							</button>
							<button onclick={() => loadSimilar(globalIdx)} class="rounded-md px-2 py-1 text-[11px] text-ink-500 hover:text-ink-200 hover:bg-ink-800/50 transition-colors flex items-center gap-1" title="类似段落" disabled={loadingSimilar.has(globalIdx)}>
								<RefreshCw size={12} class={loadingSimilar.has(globalIdx) ? 'animate-spin' : ''} />
								<span class="hidden sm:inline">类似</span>
							</button>
							{/if}
							<a href="/reading/{result.book_id || result.id}?chapter={result.chapter_index}{result.chunk_index != null ? `&chunk=${result.chunk_index}` : ''}" class="rounded-md px-2 py-1 text-[11px] text-ink-500 hover:text-accent-400 hover:bg-accent-500/10 transition-colors flex items-center gap-1" title="跳转原文">
								<ExternalLink size={12} />
								<span class="hidden sm:inline">原文</span>
							</a>
						</div>
					</div>

					<!-- Context expansion -->
					{#if isExpanded}
						<div class="border-t border-ink-800/30 bg-ink-950/30 px-5 py-3 space-y-2">
							{#if loadingContext.has(globalIdx)}
								<div class="flex items-center gap-2 text-xs text-ink-500"><RefreshCw size={12} class="animate-spin" /> 加载上下文...</div>
							{:else if contextData.has(globalIdx)}
								{@const chunks = contextData.get(globalIdx) || []}
								{#if chunks.length > 0}
									{#each chunks as chunk}
										<div class="rounded-lg px-3 py-2 text-sm leading-relaxed {chunk.is_target ? 'bg-accent-500/5 border border-accent-500/20 text-ink-200' : 'text-ink-400 bg-ink-900/50'}">
											<span class="text-[10px] {chunk.is_target ? 'text-accent-400 font-medium' : 'text-ink-600'}">{chunk.is_target ? '◆ 当前' : `#${chunk.chunk_index}`}</span>
											<p class="mt-0.5">{chunk.content}</p>
										</div>
									{/each}
								{:else}
									<p class="text-xs text-ink-500 italic">该段落暂无周围上下文数据</p>
								{/if}
							{/if}
						</div>
					{/if}

					<!-- Similar results -->
					{#if similarResults.has(globalIdx)}
						{@const similar = similarResults.get(globalIdx) || []}
						{#if similar.length > 0}
							<div class="border-t border-ink-800/30 bg-ink-950/20 px-5 py-3">
								<p class="text-[10px] text-ink-500 font-medium uppercase tracking-wide mb-2">类似段落</p>
								<div class="space-y-1.5">
									{#each similar.slice(0, 3) as sim}
										<div class="rounded-lg bg-ink-900/50 px-3 py-2 text-xs text-ink-400 leading-relaxed line-clamp-2">
											<span class="text-ink-500 font-medium">{sim.book_title || '—'} ›</span> {sim.content_snippet || sim.content || ''}
										</div>
									{/each}
								</div>
							</div>
						{/if}
					{/if}
				</div>
				{/each}
					</div>

					<!-- Expand/collapse button -->
					{#if bookResults.length > 3}
						<div class="border-t border-ink-800/30 px-5 py-2.5">
							<button onclick={() => toggleBookGroup(bookTitle)} class="flex items-center gap-1.5 text-xs text-ink-400 hover:text-ink-200 transition-colors">
								{#if showAll}
									<ChevronDown size={14} /> 收起
								{:else}
									<ChevronRight size={14} /> 展开剩余 {bookResults.length - 3} 条结果
								{/if}
							</button>
						</div>
					{/if}
				</div>
			{/each}
		</div>

		<!-- Search error -->
		{:else if searchError}
			<div class="py-12 text-center space-y-4">
				<div class="mx-auto flex h-16 w-16 items-center justify-center rounded-2xl bg-error/10 text-error">
					<X size={28} strokeWidth={1.5} />
				</div>
				<div>
					<p class="text-ink-100 text-lg">搜索暂时不可用</p>
					<p class="mt-1 text-sm text-ink-500">{searchError}</p>
				</div>
				<button
					onclick={performSearch}
					class="inline-flex items-center gap-2 rounded-lg border border-ink-700/60 bg-ink-900/60 px-4 py-2 text-sm text-ink-200 transition-colors hover:border-accent-500/30 hover:text-accent-300"
				>
					<RefreshCw size={15} />
					重试搜索
				</button>
			</div>

		<!-- No results -->
		{:else if query && !loading}
		<div class="py-12 text-center space-y-4">
			<div class="mx-auto flex h-16 w-16 items-center justify-center rounded-2xl bg-ink-800/50">
				<Search size={28} strokeWidth={1.5} class="text-ink-500" />
			</div>
			<p class="text-ink-300 text-lg">没有找到 "{query}" 相关结果</p>
			<div class="text-sm text-ink-500 space-y-1">
				<p>• 尝试更通用的关键词</p>
				{#if searchMode !== 'semantic'}
					<p>• <button onclick={() => { searchMode = 'semantic'; performSearch(); }} class="text-accent-400 hover:underline">切换到语义搜索</button></p>
				{/if}
				<p>• 确认书籍已完成 AI 分析处理</p>
			</div>
			{#if searchMode !== 'semantic'}
				<button onclick={() => { searchMode = 'semantic'; performSearch(); }} class="mt-3 inline-flex items-center gap-2 rounded-lg border border-accent-500/20 bg-accent-500/5 px-4 py-2 text-sm text-accent-400 hover:bg-accent-500/10 transition-colors">
					<Brain size={16} /> 尝试语义搜索
				</button>
			{/if}
		</div>

	<!-- Empty state -->
	{:else}
		<div class="py-10 text-center space-y-6">
			<div class="flex flex-col items-center gap-3">
				<div class="flex h-16 w-16 items-center justify-center rounded-2xl bg-gradient-to-br from-accent-500/20 to-violet-500/20">
					<Sparkles size={28} class="text-accent-400" />
				</div>
				<p class="text-sm text-ink-400">试试这些搜索：</p>
			</div>
			<div class="flex flex-wrap justify-center gap-2 max-w-lg mx-auto">
				{#each ['主角的师父是谁', '关于修炼体系的设定', '主角与反派的对话', '结局是什么', '女主出场描写', '最精彩的战斗'] as suggestion}
					<button onclick={() => { query = suggestion; performSearch(); }} class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-3.5 py-2 text-sm text-ink-300 hover:border-accent-500/30 hover:text-accent-400 hover:bg-accent-500/5 transition-all">{suggestion}</button>
				{/each}
			</div>
			{#if searchHistory.length > 0}
				<div class="pt-4 border-t border-ink-800/30">
					<p class="text-xs text-ink-500 mb-2">最近搜索</p>
					<div class="flex flex-wrap justify-center gap-1.5">
						{#each searchHistory.slice(0, 5) as item}
							<button onclick={() => selectHistoryItem(item)} class="flex items-center gap-1.5 rounded-md bg-ink-800/40 px-2.5 py-1 text-xs text-ink-400 hover:text-ink-200 transition-colors">
								<Clock size={10} class="text-ink-600" /> {item}
							</button>
						{/each}
					</div>
				</div>
			{/if}
		</div>
	{/if}
</div>

<style>
	:global(.search-highlight mark) {
		background-color: rgb(251 191 36 / 0.2);
		color: rgb(253 230 138);
		border-radius: 2px;
		padding: 0 2px;
		font-weight: 500;
	}
</style>
