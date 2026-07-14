<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { goto } from '$app/navigation';
	import { ArrowRight, BookOpen, Compass, Layers, ListChecks, RefreshCw, Search, Sparkles, Tag, TrendingUp, X } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';

	type RecommendationBook = {
		id: string;
		title: string;
		author: string | null;
		cover_path: string | null;
		score: number;
		match_reason: string;
		semantic_anchors?: string[];
		semantic_anchor_count?: number;
		similarity_score?: number;
		recommendation_score?: number;
	};

	type RecommendationGroup = {
		id: string;
		category: string;
		reason: string;
		books: RecommendationBook[];
	};

	type ReadingQueueItem = {
		id: string;
		title: string;
		author: string | null;
		reading_status: string;
		priority: 'high' | 'medium' | 'low';
		reason: string;
	};

	let groups = $state<RecommendationGroup[]>([]);
	let queueItems = $state<ReadingQueueItem[]>([]);
	let queueTotal = $state(0);
	let loading = $state(true);
	let error = $state<string | null>(null);
	let activeCategory = $state<string | null>($page.url.searchParams.get('category'));

	let categories = $derived([...new Set(groups.map((group) => group.category))]);
	let filteredGroups = $derived(
		activeCategory ? groups.filter((group) => group.category === activeCategory) : groups
	);

	$effect(() => {
		const params = new URLSearchParams($page.url.searchParams);
		if (activeCategory) params.set('category', activeCategory);
		else params.delete('category');
		const qs = params.toString();
		const nextUrl = qs ? `/discover?${qs}` : '/discover';
		if (nextUrl !== $page.url.pathname + $page.url.search) {
			goto(nextUrl, { replaceState: true, keepFocus: true, noScroll: true });
		}
	});

	onMount(loadRecommendations);

	async function loadRecommendations() {
		loading = true;
		error = null;
		try {
			const [recommendationGroups, readingQueue] = await Promise.all([
				api.getRecommendations(),
				api.getReadingQueue().catch(() => ({ queue: [], total: 0 })),
			]);
			groups = recommendationGroups;
			queueItems = readingQueue.queue.slice(0, 4);
			queueTotal = readingQueue.total;
		} catch (err) {
			error = err instanceof Error ? err.message : '探索内容加载失败';
			groups = [];
			queueItems = [];
			queueTotal = 0;
		} finally {
			loading = false;
		}
	}

	function normalizeCoverPath(path: string | null): string | null {
		if (!path) return null;
		if (path.startsWith('/api/') || path.startsWith('http') || path.startsWith('data:')) return path;
		return `/api/covers/${path}`;
	}

	function scorePercent(score: number): number {
		return Math.round(Math.max(0, Math.min(1, score)) * 100);
	}

	function priorityLabel(priority: ReadingQueueItem['priority']): string {
		if (priority === 'high') return '优先';
		if (priority === 'low') return '稍后';
		return '下一本';
	}

	async function dismissBook(bookId: string) {
		const previousGroups = groups;
		groups = groups
			.map((group) => ({ ...group, books: group.books.filter((book) => book.id !== bookId) }))
			.filter((group) => group.books.length > 0);
		try {
			await api.submitRecommendationFeedback(bookId, 'not_interested');
			toast.success('已记录「不感兴趣」', {
				action: {
					label: '撤销',
					onClick: async () => {
						await api.clearRecommendationFeedback(bookId).catch(() => {});
						await loadRecommendations();
					},
				},
			});
		} catch {
			groups = previousGroups;
			toast.error('操作失败，请稍后重试');
		}
	}
</script>

<svelte:head>
	<title>Nova Reader — 探索</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<div class="flex flex-wrap items-center justify-between gap-4">
		<div class="flex items-center gap-3">
			<div class="flex h-12 w-12 items-center justify-center rounded-xl bg-accent-500/10 text-accent-400 ring-1 ring-accent-500/20">
				<Compass size={24} strokeWidth={1.8} />
			</div>
			<div>
				<h1 class="text-2xl font-bold text-ink-50">探索</h1>
				<p class="mt-1 text-sm text-ink-400">基于阅读进度、标签、相似内容和书库结构发现下一本书</p>
			</div>
		</div>

		<div class="flex items-center gap-2">
			{#if categories.length > 0}
				<div
					class="flex max-w-full gap-1 overflow-x-auto rounded-lg bg-ink-900/50 p-1 md:max-w-none"
					role="tablist"
					aria-label="探索分类"
				>
					<button
						type="button"
						onclick={() => activeCategory = null}
						role="tab"
						aria-selected={activeCategory === null}
						class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {activeCategory === null ? 'bg-accent-500/20 text-accent-300' : 'text-ink-400 hover:text-ink-200'}"
					>
						全部
					</button>
					{#each categories as category}
						<button
							type="button"
							onclick={() => activeCategory = category}
							role="tab"
							aria-selected={activeCategory === category}
							class="rounded-md px-3 py-1.5 text-xs font-medium transition-colors focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70 {activeCategory === category ? 'bg-accent-500/20 text-accent-300' : 'text-ink-400 hover:text-ink-200'}"
						>
							{category}
						</button>
					{/each}
				</div>
			{/if}
			<button
				onclick={loadRecommendations}
				class="inline-flex items-center gap-2 rounded-lg bg-accent-500 px-3 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300/80 disabled:opacity-50"
				disabled={loading}
			>
				<RefreshCw size={15} class={loading ? 'animate-spin' : ''} />
				刷新
			</button>
		</div>
	</div>

	<nav aria-label="探索入口" class="flex gap-2 overflow-x-auto pb-1">
		<a
			href="/search"
			class="inline-flex shrink-0 items-center gap-2 rounded-lg border border-ink-700/50 px-3 py-2 text-sm text-ink-300 transition-colors hover:bg-ink-800/50 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
		>
			<Search size={15} />
			语义搜索
		</a>
		<a
			href="/semantic-tags?tab=vibe"
			class="inline-flex shrink-0 items-center gap-2 rounded-lg border border-ink-700/50 px-3 py-2 text-sm text-ink-300 transition-colors hover:bg-ink-800/50 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
		>
			<Sparkles size={15} />
			氛围检索
		</a>
		<a
			href="/library"
			class="inline-flex shrink-0 items-center gap-2 rounded-lg border border-ink-700/50 px-3 py-2 text-sm text-ink-300 transition-colors hover:bg-ink-800/50 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
		>
			<BookOpen size={15} />
			所有书籍
		</a>
		<a
			href="/series"
			class="inline-flex shrink-0 items-center gap-2 rounded-lg border border-ink-700/50 px-3 py-2 text-sm text-ink-300 transition-colors hover:bg-ink-800/50 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
		>
			<Layers size={15} />
			系列
		</a>
		<a
			href="/tags"
			class="inline-flex shrink-0 items-center gap-2 rounded-lg border border-ink-700/50 px-3 py-2 text-sm text-ink-300 transition-colors hover:bg-ink-800/50 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
		>
			<Tag size={15} />
			标签
		</a>
	</nav>

	{#if !loading && queueItems.length > 0}
		<section class="rounded-lg border border-ink-800/50 bg-ink-900/35 p-4" aria-label="阅读队列">
			<div class="mb-3 flex flex-wrap items-center justify-between gap-2">
				<div class="flex items-center gap-2">
					<ListChecks class="h-4 w-4 text-accent-400" strokeWidth={1.8} />
					<h2 class="text-sm font-semibold text-ink-100">阅读队列</h2>
				</div>
				<span class="text-xs tabular-nums text-ink-500">{queueTotal} 本</span>
			</div>

			<div class="grid gap-2 md:grid-cols-2 xl:grid-cols-4">
				{#each queueItems as item}
					<a
						href="/library/{item.id}"
						class="flex min-w-0 items-center justify-between gap-3 rounded-md border border-ink-800/50 bg-ink-950/45 px-3 py-2.5 transition-[background-color,border-color] hover:border-accent-500/25 hover:bg-ink-900 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
					>
						<div class="min-w-0">
							<p class="truncate text-sm font-medium text-ink-100">{item.title}</p>
							<p class="mt-0.5 truncate text-xs text-ink-500">{item.author || '未知作者'} · {item.reason}</p>
						</div>
						<span class="shrink-0 rounded border border-accent-500/20 bg-accent-500/10 px-2 py-1 text-[10px] font-medium text-accent-300">
							{priorityLabel(item.priority)}
						</span>
					</a>
				{/each}
			</div>
		</section>
	{/if}

	{#if loading}
		<div class="grid gap-4 lg:grid-cols-3">
			{#each Array(6) as _}
				<div class="h-44 animate-pulse rounded-xl border border-ink-800/50 bg-ink-900/40"></div>
			{/each}
		</div>
	{:else if error}
		<div class="rounded-xl border border-red-500/20 bg-red-500/5 p-8 text-center">
			<TrendingUp class="mx-auto mb-3 h-9 w-9 text-red-400" />
			<p class="text-sm text-red-300">{error}</p>
		</div>
	{:else if groups.length === 0}
		<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-12 text-center">
			<BookOpen class="mx-auto mb-4 h-12 w-12 text-ink-600" strokeWidth={1.4} />
			<h2 class="text-lg font-semibold text-ink-200">暂无探索内容</h2>
			<p class="mt-1 text-sm text-ink-500">继续阅读、打标签或完成语义索引后，这里会生成新的探索分组。</p>
			<div class="mt-5 flex flex-wrap justify-center gap-2">
				<a href="/library" aria-label="打开所有书籍补充探索数据" class="rounded-lg bg-accent-500 px-4 py-2 text-sm font-medium text-ink-950 transition-colors hover:bg-accent-400 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-300/80">浏览所有书籍</a>
				<a href="/search" aria-label="打开语义搜索探索内容" class="rounded-lg border border-ink-700/50 px-4 py-2 text-sm text-ink-300 transition-colors hover:bg-ink-800/50 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70">语义搜索</a>
				<a href="/semantic-tags?tab=vibe" aria-label="打开智能标签氛围检索" class="rounded-lg border border-ink-700/50 px-4 py-2 text-sm text-ink-300 transition-colors hover:bg-ink-800/50 hover:text-ink-100 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70">氛围检索</a>
			</div>
		</div>
	{:else}
		<div class="space-y-8">
			{#each filteredGroups as group}
				<section class="space-y-3">
					<div>
						<h2 class="text-lg font-semibold text-ink-100">{group.category}</h2>
						{#if group.reason}
							<p class="mt-0.5 max-w-3xl text-sm text-ink-500">{group.reason}</p>
						{/if}
					</div>

					<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
						{#each group.books as book}
							{@const cover = normalizeCoverPath(book.cover_path)}
							<article class="group relative">
								<button
									type="button"
									onclick={() => dismissBook(book.id)}
									class="absolute left-2 top-2 z-10 flex h-6 w-6 items-center justify-center rounded-full bg-black/60 text-ink-300 opacity-100 backdrop-blur-sm transition-[background-color,color,opacity] hover:bg-red-500/80 hover:text-white focus:opacity-100 focus:outline-none focus:ring-2 focus:ring-red-300/60 sm:opacity-0 sm:group-hover:opacity-100"
									title="不感兴趣"
									aria-label="不感兴趣"
								>
									<X size={13} />
								</button>
								<a
									href="/library/{book.id}"
									class="flex min-h-36 gap-3 rounded-lg border border-ink-800/50 bg-ink-900/70 p-3 transition-[background-color,border-color,box-shadow,transform] hover:-translate-y-0.5 hover:border-accent-500/25 hover:bg-ink-900 hover:shadow-lg hover:shadow-ink-950/20 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
								>
									<div class="h-28 w-20 shrink-0 overflow-hidden rounded-lg bg-ink-800 shadow-md ring-1 ring-white/5">
										{#if cover}
											<img src={cover} alt={book.title} width="80" height="112" loading="lazy" class="h-full w-full object-cover" />
										{:else}
											<div class="flex h-full items-center justify-center px-2 text-center text-[10px] font-semibold leading-tight text-ink-400">{book.title}</div>
										{/if}
									</div>

									<div class="flex min-w-0 flex-1 flex-col py-1">
										<h3 class="line-clamp-2 text-sm font-semibold leading-snug text-ink-100 transition-colors group-hover:text-accent-300">{book.title}</h3>
										<p class="mt-1 truncate text-xs text-ink-500">{book.author || '未知作者'}</p>
										{#if book.match_reason}
											<p class="mt-2 line-clamp-2 text-xs leading-relaxed text-ink-400">{book.match_reason}</p>
										{/if}
										<div class="mt-auto flex items-center gap-2 pt-3">
											<div class="h-1.5 flex-1 overflow-hidden rounded-full bg-ink-800">
												<div class="h-full rounded-full bg-accent-500" style="width: {scorePercent(book.score)}%"></div>
											</div>
											<span class="text-[10px] tabular-nums text-ink-500">{scorePercent(book.score)}</span>
											<ArrowRight class="h-3.5 w-3.5 text-ink-600 transition-transform group-hover:translate-x-0.5 group-hover:text-accent-400" />
										</div>
									</div>
								</a>
							</article>
						{/each}
					</div>
				</section>
			{/each}
		</div>
	{/if}
</div>
