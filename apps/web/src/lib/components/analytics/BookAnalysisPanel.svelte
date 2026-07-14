<script lang="ts">
	import { api } from '$services/api';
	import { getDeepAnalysisStatus } from '$lib/utils/analysisStatus';
	import type { DeepAnalysisState } from '$lib/utils/analysisStatus';
	import { BookText, AlertTriangle, Activity, TrendingUp, Sparkles, ChevronRight, MapPin, Clock } from 'lucide-svelte';

	let {
		bookId,
		chapterCount = null,
		refreshKey = 0,
	}: {
		bookId: string;
		chapterCount?: number | null;
		refreshKey?: number;
	} = $props();

	type Overview = Awaited<ReturnType<typeof api.getAnalysisOverview>>;
	type TaskRow = Awaited<ReturnType<typeof api.getTaskQueue>>['data'][number];
	type Summary = Awaited<ReturnType<typeof api.getChapterSummaries>>[number];
	type Foreshadowing = Awaited<ReturnType<typeof api.getForeshadowing>>['entries'][number];
	type StateChange = Awaited<ReturnType<typeof api.getStateChanges>>[number];
	type SentimentPoint = Awaited<ReturnType<typeof api.getSentimentArc>>['data'][number];

	let overview = $state<Overview | null>(null);
	let tasks = $state<TaskRow[]>([]);
	let summaries = $state<Summary[]>([]);
	let foreshadowing = $state<Foreshadowing[]>([]);
	let stateChanges = $state<StateChange[]>([]);
	let sentiment = $state<SentimentPoint[]>([]);
	let loading = $state(true);
	let refreshing = $state(false);
	let loadError = $state(false);
	let loaded = $state(false);

	// Group character arcs by character name for the "角色弧线" view.
	let arcs = $derived.by(() => {
		const map = new Map<string, StateChange[]>();
		for (const sc of stateChanges) {
			const list = map.get(sc.character_name) ?? [];
			list.push(sc);
			map.set(sc.character_name, list);
		}
		return [...map.entries()]
			.map(([name, changes]) => ({ name, changes: changes.sort((a, b) => a.chapter_index - b.chapter_index) }))
			.sort((a, b) => b.changes.length - a.changes.length)
			.slice(0, 8);
	});

	let unresolved = $derived(foreshadowing.filter((f) => f.status === 'unresolved'));
	let analysisStatus = $derived(getDeepAnalysisStatus({ overview, chapterCount, tasks }));

	// Build an SVG polyline path from the stored sentiment arc (overall score in [-1, 1]).
	let sentimentPath = $derived.by(() => {
		if (sentiment.length < 2) return '';
		const w = 100;
		const h = 100;
		const n = sentiment.length;
		return sentiment
			.map((p, i) => {
				const x = (i / (n - 1)) * w;
				const y = h - ((p.overall + 1) / 2) * h;
				return `${i === 0 ? 'M' : 'L'}${x.toFixed(2)},${y.toFixed(2)}`;
			})
			.join(' ');
	});

	$effect(() => {
		void bookId;
		void refreshKey;
		loaded = false;
		load();
	});

	$effect(() => {
		if (!analysisStatus.isPolling) return;
		const timer = window.setTimeout(() => {
			load({ silent: true });
		}, 3000);
		return () => window.clearTimeout(timer);
	});

	async function load(options: { silent?: boolean } = {}) {
		if (!options.silent || !loaded) {
			loading = true;
		} else {
			refreshing = true;
		}
		loadError = false;
		try {
			const [ov, queue] = await Promise.all([
				api.getAnalysisOverview(bookId),
				api.getTaskQueue({ book_id: bookId, category: 'ai', per_page: 10 }),
			]);
			overview = ov;
			tasks = queue.data;
			if (ov.has_deep_analysis) {
				const [s, f, sc, sent] = await Promise.all([
					api.getChapterSummaries(bookId).catch(() => []),
					api.getForeshadowing(bookId).then((r) => r.entries).catch(() => []),
					api.getStateChanges(bookId).catch(() => []),
					api.getSentimentArc(bookId).then((r) => r.data).catch(() => []),
				]);
				summaries = s;
				foreshadowing = f;
				stateChanges = sc;
				sentiment = sent;
			} else {
				summaries = [];
				foreshadowing = [];
				stateChanges = [];
				sentiment = [];
			}
		} catch {
			loadError = true;
		} finally {
			loaded = true;
			loading = false;
			refreshing = false;
		}
	}

	function stateLabel(state: DeepAnalysisState): string {
		switch (state) {
			case 'queued': return '任务排队中';
			case 'running': return '任务运行中';
			case 'failed': return '任务未完成';
			case 'partial': return '结果未完整';
			case 'ready': return '结果已生成';
			default: return '等待生成';
		}
	}

	function stateClass(state: DeepAnalysisState): string {
		switch (state) {
			case 'queued':
			case 'running':
				return 'border-accent-500/25 bg-accent-500/10 text-accent-300';
			case 'failed':
				return 'border-red-500/25 bg-red-500/10 text-red-300';
			case 'partial':
				return 'border-amber-500/25 bg-amber-500/10 text-amber-300';
			case 'ready':
				return 'border-emerald-500/25 bg-emerald-500/10 text-emerald-300';
			default:
				return 'border-ink-800/70 bg-ink-900/30 text-ink-400';
		}
	}

	function sentimentBadge(sentiment?: string | null): string {
		switch (sentiment) {
			case 'positive': return 'bg-emerald-500/10 text-emerald-400';
			case 'negative': return 'bg-red-500/10 text-red-400';
			case 'tense': return 'bg-amber-500/10 text-amber-400';
			default: return 'bg-ink-800 text-ink-400';
		}
	}
</script>

{#if loading}
	<div class="space-y-3">
		{#each Array(3) as _}
			<div class="h-24 animate-pulse rounded-xl bg-ink-900/50"></div>
		{/each}
	</div>
{:else if loadError}
	<div class="rounded-xl border border-ink-800/60 bg-ink-900/30 p-6 text-sm text-ink-500">分析结果加载失败，请稍后重试。</div>
{:else if !overview?.has_deep_analysis}
	<div class="space-y-4">
		<div class="rounded-xl border p-4 {stateClass(analysisStatus.state)}">
			<div class="flex items-center justify-between gap-3">
				<div class="min-w-0">
					<p class="flex items-center gap-2 text-sm font-medium">
						<Activity size={15} class={analysisStatus.isPolling ? 'animate-pulse' : ''} />
						{stateLabel(analysisStatus.state)}
						{#if refreshing}
							<span class="text-[10px] opacity-70">刷新中</span>
						{/if}
					</p>
					<p class="mt-1 text-xs opacity-80">{analysisStatus.message}</p>
				</div>
				<span class="shrink-0 text-sm font-semibold">{analysisStatus.progress}%</span>
			</div>
			<div class="mt-3 h-1.5 overflow-hidden rounded-full bg-ink-950/50">
				<div class="h-full rounded-full bg-current transition-all" style="width: {analysisStatus.progress}%"></div>
			</div>
			{#if analysisStatus.missingLabels.length > 0}
				<div class="mt-3 flex flex-wrap gap-1.5">
					{#each analysisStatus.missingLabels as label}
						<span class="rounded-full bg-ink-950/40 px-2 py-0.5 text-[10px] opacity-85">{label}</span>
					{/each}
				</div>
			{/if}
		</div>
		<div class="rounded-xl border border-dashed border-ink-800/70 bg-ink-900/20 p-8 text-center">
			<Sparkles class="mx-auto mb-3 h-7 w-7 text-ink-600" />
			<p class="text-sm text-ink-300">尚未生成深度分析结果</p>
			<p class="mt-1 text-xs text-ink-500">章节摘要、情感曲线、伏笔与角色弧线会在任务完成后显示在这里。</p>
		</div>
	</div>
{:else}
	<div class="space-y-6">
		<div class="rounded-xl border p-4 {stateClass(analysisStatus.state)}">
			<div class="flex items-center justify-between gap-3">
				<div class="min-w-0">
					<p class="flex items-center gap-2 text-sm font-medium">
						<Activity size={15} class={analysisStatus.isPolling ? 'animate-pulse' : ''} />
						{stateLabel(analysisStatus.state)}
						{#if refreshing}
							<span class="text-[10px] opacity-70">刷新中</span>
						{/if}
					</p>
					<p class="mt-1 text-xs opacity-80">{analysisStatus.message}</p>
				</div>
				<span class="shrink-0 text-sm font-semibold">{analysisStatus.progress}%</span>
			</div>
			<div class="mt-3 h-1.5 overflow-hidden rounded-full bg-ink-950/50">
				<div class="h-full rounded-full bg-current transition-all" style="width: {analysisStatus.progress}%"></div>
			</div>
			{#if analysisStatus.missingLabels.length > 0}
				<div class="mt-3 flex flex-wrap gap-1.5">
					{#each analysisStatus.missingLabels as label}
						<span class="rounded-full bg-ink-950/40 px-2 py-0.5 text-[10px] opacity-85">{label}</span>
					{/each}
				</div>
			{/if}
		</div>

		<!-- Stat strip -->
		<div class="grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
			<div class="rounded-xl border border-ink-800/60 bg-ink-900/35 p-4">
				<BookText size={16} class="mb-2 text-accent-300" />
				<p class="text-xs text-ink-500">章节摘要</p>
				<p class="mt-0.5 text-lg font-semibold text-ink-100">{overview.chapter_summaries}</p>
			</div>
			<div class="rounded-xl border border-ink-800/60 bg-ink-900/35 p-4">
				<Activity size={16} class="mb-2 text-cyan-300" />
				<p class="text-xs text-ink-500">情感曲线节点</p>
				<p class="mt-0.5 text-lg font-semibold text-ink-100">{overview.sentiment_arcs}</p>
			</div>
			<div class="rounded-xl border border-ink-800/60 bg-ink-900/35 p-4">
				<AlertTriangle size={16} class="mb-2 text-amber-300" />
				<p class="text-xs text-ink-500">未回收伏笔</p>
				<p class="mt-0.5 text-lg font-semibold text-ink-100">{overview.foreshadowing_unresolved} / {overview.foreshadowing_total}</p>
			</div>
			<div class="rounded-xl border border-ink-800/60 bg-ink-900/35 p-4">
				<TrendingUp size={16} class="mb-2 text-purple-300" />
				<p class="text-xs text-ink-500">宏观分析窗口</p>
				<p class="mt-0.5 text-lg font-semibold text-ink-100">{overview.macro_windows}</p>
			</div>
		</div>

		<!-- Sentiment arc -->
		{#if sentiment.length > 1}
			<div class="rounded-xl border border-ink-800/60 bg-ink-900/30 p-5">
				<h4 class="mb-4 flex items-center gap-2 text-sm font-medium text-ink-200"><Activity size={16} /> 情感曲线</h4>
				<div class="relative">
					<svg viewBox="0 0 100 100" preserveAspectRatio="none" class="h-32 w-full">
						<line x1="0" y1="50" x2="100" y2="50" stroke="currentColor" stroke-width="0.3" class="text-ink-700" stroke-dasharray="2 2" />
						<path d={sentimentPath} fill="none" stroke="url(#sentGrad)" stroke-width="1.2" vector-effect="non-scaling-stroke" />
						<defs>
							<linearGradient id="sentGrad" x1="0" y1="0" x2="1" y2="0">
								<stop offset="0%" stop-color="#06b6d4" />
								<stop offset="100%" stop-color="#8b5cf6" />
							</linearGradient>
						</defs>
					</svg>
					<div class="mt-1 flex justify-between text-[10px] text-ink-600">
						<span>第 1 章</span>
						<span>负向 ↓ · 正向 ↑</span>
						<span>第 {sentiment.length} 章</span>
					</div>
				</div>
			</div>
		{/if}

		<div class="grid gap-4 lg:grid-cols-2">
			<!-- Risk / unresolved foreshadowing -->
			<div class="rounded-xl border border-ink-800/60 bg-ink-900/30 p-5">
				<h4 class="mb-4 flex items-center gap-2 text-sm font-medium text-ink-200"><AlertTriangle size={16} class="text-amber-400" /> 风险提示 · 未回收伏笔</h4>
				{#if unresolved.length === 0}
					<p class="text-sm text-ink-500">没有检测到未回收的伏笔。</p>
				{:else}
					<ul class="space-y-3">
						{#each unresolved.slice(0, 8) as f}
							<li class="rounded-lg border border-amber-500/15 bg-amber-500/5 p-3">
								<div class="flex items-center justify-between gap-2">
									<span class="rounded-full bg-amber-500/10 px-2 py-0.5 text-[10px] text-amber-300">第 {f.setup_chapter} 章埋设</span>
									<span class="text-[10px] text-ink-500">{f.category}</span>
								</div>
								<p class="mt-2 text-sm leading-relaxed text-ink-300">{f.setup_description}</p>
							</li>
						{/each}
					</ul>
				{/if}
			</div>

			<!-- Character arcs -->
			<div class="rounded-xl border border-ink-800/60 bg-ink-900/30 p-5">
				<h4 class="mb-4 flex items-center gap-2 text-sm font-medium text-ink-200"><TrendingUp size={16} class="text-purple-400" /> 角色弧线</h4>
				{#if arcs.length === 0}
					<p class="text-sm text-ink-500">暂无角色状态变化记录。</p>
				{:else}
					<div class="space-y-4">
						{#each arcs as arc}
							<div>
								<p class="mb-2 text-sm font-medium text-ink-100">{arc.name}</p>
								<div class="space-y-1.5">
									{#each arc.changes.slice(0, 5) as change}
										<div class="flex items-center gap-2 text-xs text-ink-400">
											<span class="shrink-0 rounded bg-ink-800 px-1.5 py-0.5 text-[10px] text-ink-500">第 {change.chapter_index} 章</span>
											{#if change.from_state}
												<span class="text-ink-500">{change.from_state}</span>
												<ChevronRight size={11} class="shrink-0 text-ink-600" />
											{/if}
											<span class="text-ink-200">{change.to_state}</span>
										</div>
									{/each}
								</div>
							</div>
						{/each}
					</div>
				{/if}
			</div>
		</div>

		<!-- Chapter summaries -->
		{#if summaries.length > 0}
			<div class="rounded-xl border border-ink-800/60 bg-ink-900/30 p-5">
				<h4 class="mb-4 flex items-center gap-2 text-sm font-medium text-ink-200"><BookText size={16} /> 章节摘要</h4>
				<div class="space-y-3 max-h-[28rem] overflow-y-auto pr-1">
					{#each summaries as s}
						<div class="rounded-lg border border-ink-800/50 bg-ink-950/30 p-3">
							<div class="flex flex-wrap items-center gap-2">
								<span class="rounded bg-ink-800 px-1.5 py-0.5 text-[10px] text-ink-400">第 {s.chapter_index + 1} 章</span>
								{#if s.sentiment}
									<span class="rounded-full px-2 py-0.5 text-[10px] {sentimentBadge(s.sentiment)}">{s.sentiment}</span>
								{/if}
								{#if s.time_marker}
									<span class="flex items-center gap-1 text-[10px] text-ink-500"><Clock size={10} /> {s.time_marker}</span>
								{/if}
								{#if s.location}
									<span class="flex items-center gap-1 text-[10px] text-ink-500"><MapPin size={10} /> {s.location}</span>
								{/if}
							</div>
							<p class="mt-2 text-sm leading-relaxed text-ink-300">{s.summary}</p>
							{#if s.key_event}
								<p class="mt-1.5 text-xs text-accent-300/90">关键事件：{s.key_event}</p>
							{/if}
							{#if s.characters_present?.length}
								<div class="mt-2 flex flex-wrap gap-1">
									{#each s.characters_present.slice(0, 6) as c}
										<span class="rounded bg-ink-800/60 px-1.5 py-0.5 text-[10px] text-ink-400">{c}</span>
									{/each}
								</div>
							{/if}
						</div>
					{/each}
				</div>
			</div>
		{/if}
	</div>
{/if}
