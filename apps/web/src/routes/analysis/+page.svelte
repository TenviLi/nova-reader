<script lang="ts">
	import { api } from '$services/api';
	import { getDeepAnalysisStatus } from '$lib/utils/analysisStatus';
	import type { DeepAnalysisState } from '$lib/utils/analysisStatus';
	import { onMount } from 'svelte';
	import { page } from '$app/stores';
	import { LineChart, BookOpen, Eye, TrendingUp, Sparkles, AlertTriangle, Layers, Activity } from 'lucide-svelte';

	interface SentimentPoint {
		chapter: number;
		overall: number;
		is_peak?: boolean;
		is_valley?: boolean;
		label?: string;
	}

	interface AnalysisOverview {
		has_deep_analysis: boolean;
		chapter_count?: number;
		word_count?: number;
		chapter_summaries: number;
		sentiment_arcs: number;
		foreshadowing_total: number;
		foreshadowing_unresolved: number;
		macro_windows: number;
	}

	interface SentimentAnalysis {
		data: SentimentPoint[];
		stats: {
			average_score: number;
			peaks: SentimentPoint[];
			valleys: SentimentPoint[];
			total_chapters: number;
		};
	}

	interface ForeshadowingEntry {
		id: string;
		status: 'resolved' | 'unresolved';
		setup_chapter: number;
		setup_description: string;
		payoff_chapter?: number;
		payoff_description?: string;
		category: string;
	}

	interface ForeshadowingAnalysis {
		entries: ForeshadowingEntry[];
	}

	interface MacroItem {
		id: string;
		start_chapter: number;
		end_chapter: number;
		theme: string;
		summary: string;
		plot_arc?: string;
		arc_summary?: string;
		key_conflicts: string[];
	}

	interface ChapterSummary {
		id: string;
		chapter_index: number;
		title?: string;
		summary: string;
		time_marker?: string;
		location?: string;
		sentiment?: string;
		key_event?: string;
	}

	type TaskRow = Awaited<ReturnType<typeof api.getTaskQueue>>['data'][number];

	let bookId = $derived($page.url.searchParams.get('book_id') || '');
	let books = $state<Array<{ id: string; title: string }>>([]);
	let selectedBookId = $state('');
	let loading = $state(false);
	let submitting = $state(false);

	// Data
	let overview = $state<AnalysisOverview | null>(null);
	let sentimentData = $state<SentimentAnalysis | null>(null);
	let foreshadowing = $state<ForeshadowingAnalysis | null>(null);
	let macroAnalysis = $state<MacroItem[]>([]);
	let summaries = $state<ChapterSummary[]>([]);
	let tasks = $state<TaskRow[]>([]);
	let analysisStatus = $derived(getDeepAnalysisStatus({
		overview,
		chapterCount: overview?.chapter_count ?? null,
		tasks,
	}));

	let activeTab = $state<'sentiment' | 'foreshadowing' | 'macro' | 'summaries'>('sentiment');

	onMount(async () => {
		try {
			const result = await api.getBooks();
			books = result.data?.map((b) => ({ id: b.id, title: b.title })) ?? [];
			if (bookId && books.find(b => b.id === bookId)) {
				selectedBookId = bookId;
				await loadAnalysis();
			}
		} catch { /* ignore */ }
	});

	async function loadAnalysis() {
		if (!selectedBookId) return;
		loading = true;
		try {
			const [nextOverview, queue] = await Promise.all([
				api.get<AnalysisOverview>(`/analysis/${selectedBookId}/overview`),
				api.getTaskQueue({ book_id: selectedBookId, category: 'ai', per_page: 10 }),
			]);
			overview = nextOverview;
			tasks = queue.data;
			if (overview.has_deep_analysis) {
				const [sent, fore, macro, summ] = await Promise.all([
					api.get<SentimentAnalysis>(`/analysis/${selectedBookId}/sentiment`),
					api.get<ForeshadowingAnalysis>(`/analysis/${selectedBookId}/foreshadowing`),
					api.get<MacroItem[]>(`/analysis/${selectedBookId}/macro`),
					api.get<ChapterSummary[]>(`/analysis/${selectedBookId}/summaries`),
				]);
				sentimentData = sent;
				foreshadowing = fore;
				macroAnalysis = macro;
				summaries = summ;
			} else {
				sentimentData = null;
				foreshadowing = null;
				macroAnalysis = [];
				summaries = [];
			}
		} catch {
			overview = null;
			tasks = [];
		} finally {
			loading = false;
		}
	}

	async function triggerDeepAnalysis() {
		if (!selectedBookId) return;
		submitting = true;
		try {
			await api.submitPipeline(selectedBookId, 'deep_analysis');
			await loadAnalysis();
		} catch { /* ignore */ }
		finally {
			submitting = false;
		}
	}

	function selectBook() {
		loadAnalysis();
	}

	// Simple SVG sparkline for sentiment
	function sentimentPath(data: SentimentPoint[]): string {
		if (!data || data.length < 2) return '';
		const width = 800;
		const height = 120;
		const padding = 10;
		const xStep = (width - padding * 2) / (data.length - 1);
		const points = data.map((d, i) => {
			const x = padding + i * xStep;
			const y = height / 2 - (d.overall * (height / 2 - padding));
			return `${x},${y}`;
		});
		return `M ${points.join(' L ')}`;
	}

	$effect(() => {
		if (!selectedBookId || !analysisStatus.isPolling) return;
		const timer = window.setTimeout(loadAnalysis, 3000);
		return () => window.clearTimeout(timer);
	});

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
</script>

<svelte:head>
	<title>Nova Reader — 深度分析</title>
</svelte:head>

<div class="mx-auto max-w-6xl px-6 py-6 space-y-6">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div class="flex items-center gap-3">
			<Sparkles size={20} class="text-accent-400" />
			<h1 class="text-xl font-semibold text-ink-100">深度分析</h1>
			<span class="text-sm text-ink-500">情感弧线 · 伏笔追踪 · 宏观脉络</span>
		</div>
	</div>

	<!-- Book Selector -->
	<div class="flex items-center gap-3">
		<select
			bind:value={selectedBookId}
			onchange={selectBook}
			class="rounded-lg border border-ink-700/50 bg-ink-900/50 px-4 py-2 text-sm text-ink-200 focus:border-accent-500/50 focus:outline-none min-w-[200px]"
		>
			<option value="">选择书籍...</option>
			{#each books as book}
				<option value={book.id}>{book.title}</option>
			{/each}
		</select>

		{#if selectedBookId && (!analysisStatus.isPolling || analysisStatus.state === 'failed')}
			<button
				onclick={triggerDeepAnalysis}
				disabled={submitting}
				class="rounded-lg bg-accent-500/10 border border-accent-500/20 px-4 py-2 text-sm text-accent-400 hover:bg-accent-500/20 transition-colors disabled:opacity-50"
			>
				<Sparkles size={14} class="inline mr-1" /> {submitting ? '提交中...' : '启动深度分析'}
			</button>
		{/if}
	</div>

	{#if selectedBookId && overview}
		<div class="rounded-xl border p-4 {stateClass(analysisStatus.state)}">
			<div class="flex items-center justify-between gap-3">
				<div class="min-w-0">
					<p class="flex items-center gap-2 text-sm font-medium">
						<Activity size={15} class={analysisStatus.isPolling ? 'animate-pulse' : ''} />
						{stateLabel(analysisStatus.state)}
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
	{/if}

	{#if loading}
		<div class="flex items-center justify-center py-16">
			<div class="h-8 w-8 border-2 border-accent-500/30 border-t-accent-500 rounded-full animate-spin"></div>
		</div>
	{:else if !selectedBookId}
		<div class="flex flex-col items-center justify-center py-16 text-center">
			<BookOpen size={48} class="text-ink-700 mb-3" strokeWidth={1} />
			<p class="text-ink-500">选择一本书来查看深度分析结果</p>
		</div>
	{:else if overview && !overview.has_deep_analysis}
		<div class="rounded-xl border border-ink-800/50 bg-ink-900/20 p-8 text-center">
			<AlertTriangle size={32} class="mx-auto text-amber-500/70 mb-3" />
			<p class="text-ink-300">尚未进行深度分析</p>
			<p class="text-ink-500 text-sm mt-1">点击"启动深度分析"运行 Micro-Macro 滑动窗口分析</p>
		</div>
	{:else if overview}
		<!-- Stats Cards -->
		<div class="grid grid-cols-2 md:grid-cols-5 gap-3">
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/20 p-3 text-center">
				<div class="text-lg font-semibold text-ink-100">{overview.chapter_summaries}</div>
				<div class="text-[10px] text-ink-500">章节摘要</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/20 p-3 text-center">
				<div class="text-lg font-semibold text-ink-100">{overview.sentiment_arcs}</div>
				<div class="text-[10px] text-ink-500">情感数据点</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/20 p-3 text-center">
				<div class="text-lg font-semibold text-amber-400">{overview.foreshadowing_unresolved}</div>
				<div class="text-[10px] text-ink-500">未解伏笔</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/20 p-3 text-center">
				<div class="text-lg font-semibold text-green-400">{overview.foreshadowing_total - overview.foreshadowing_unresolved}</div>
				<div class="text-[10px] text-ink-500">已揭伏笔</div>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/20 p-3 text-center">
				<div class="text-lg font-semibold text-ink-100">{overview.macro_windows}</div>
				<div class="text-[10px] text-ink-500">宏观窗口</div>
			</div>
		</div>

		<!-- Tab Navigation -->
		<div class="flex border-b border-ink-800/50">
			{#each [
				{ key: 'sentiment', label: '情感曲线', icon: TrendingUp },
				{ key: 'foreshadowing', label: '伏笔追踪', icon: Eye },
				{ key: 'macro', label: '宏观脉络', icon: Layers },
				{ key: 'summaries', label: '章节摘要', icon: BookOpen },
			] as tab (tab.key)}
				<button
					onclick={() => activeTab = tab.key as typeof activeTab}
					class="flex items-center gap-1.5 px-4 py-2.5 text-sm border-b-2 transition-colors {activeTab === tab.key ? 'border-accent-500 text-accent-400' : 'border-transparent text-ink-500 hover:text-ink-300'}"
				>
					<tab.icon size={14} />
					{tab.label}
				</button>
			{/each}
		</div>

		<!-- Tab Content -->
		{#if activeTab === 'sentiment' && sentimentData}
			<div class="space-y-4">
				<!-- Sparkline Chart -->
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/20 p-4">
					<h3 class="text-sm font-medium text-ink-200 mb-3">情感走势 (整体分数)</h3>
					<svg viewBox="0 0 800 120" class="w-full h-24">
						<line x1="10" y1="60" x2="790" y2="60" stroke="currentColor" class="text-ink-800" stroke-width="0.5" stroke-dasharray="4"/>
						<path d={sentimentPath(sentimentData.data)} fill="none" stroke="currentColor" class="text-accent-400" stroke-width="2"/>
						{#each sentimentData.data.filter((d) => d.is_peak) as peak}
							<circle cx={10 + (sentimentData.data.indexOf(peak)) * (780 / (sentimentData.data.length - 1))} cy={60 - peak.overall * 50} r="4" class="fill-green-400"/>
						{/each}
						{#each sentimentData.data.filter((d) => d.is_valley) as valley}
							<circle cx={10 + (sentimentData.data.indexOf(valley)) * (780 / (sentimentData.data.length - 1))} cy={60 - valley.overall * 50} r="4" class="fill-red-400"/>
						{/each}
					</svg>
					<div class="flex justify-between text-[10px] text-ink-600 mt-1">
						<span>第1章</span>
						<span>第{sentimentData.data.length}章</span>
					</div>
				</div>

				<!-- Emotion breakdown -->
				<div class="grid grid-cols-2 md:grid-cols-4 gap-3">
					<div class="rounded-lg border border-ink-800/50 bg-ink-900/20 p-3">
						<div class="text-xs text-ink-500">平均情感</div>
						<div class="text-sm font-medium text-ink-200 mt-1">{sentimentData.stats.average_score.toFixed(2)}</div>
					</div>
					<div class="rounded-lg border border-ink-800/50 bg-ink-900/20 p-3">
						<div class="text-xs text-ink-500">高潮点</div>
						<div class="text-sm font-medium text-green-400 mt-1">{sentimentData.stats.peaks.length} 个</div>
					</div>
					<div class="rounded-lg border border-ink-800/50 bg-ink-900/20 p-3">
						<div class="text-xs text-ink-500">低谷点</div>
						<div class="text-sm font-medium text-red-400 mt-1">{sentimentData.stats.valleys.length} 个</div>
					</div>
					<div class="rounded-lg border border-ink-800/50 bg-ink-900/20 p-3">
						<div class="text-xs text-ink-500">总章节</div>
						<div class="text-sm font-medium text-ink-200 mt-1">{sentimentData.stats.total_chapters}</div>
					</div>
				</div>
			</div>

		{:else if activeTab === 'foreshadowing' && foreshadowing}
			<div class="space-y-3">
				{#each foreshadowing.entries as entry (entry.id)}
					<div class="rounded-xl border border-ink-800/50 bg-ink-900/20 p-4">
						<div class="flex items-start justify-between">
							<div class="flex-1">
								<div class="flex items-center gap-2">
									<span class="text-xs px-1.5 py-0.5 rounded {entry.status === 'resolved' ? 'bg-green-500/10 text-green-400' : 'bg-amber-500/10 text-amber-400'}">
										{entry.status === 'resolved' ? '已揭示' : '未解'}
									</span>
									<span class="text-xs text-ink-500">第{entry.setup_chapter}章</span>
									{#if entry.payoff_chapter}
										<span class="text-xs text-ink-600">→ 第{entry.payoff_chapter}章</span>
									{/if}
								</div>
								<p class="text-sm text-ink-200 mt-1.5">{entry.setup_description}</p>
								{#if entry.payoff_description}
									<p class="text-xs text-ink-400 mt-1">揭示: {entry.payoff_description}</p>
								{/if}
							</div>
							<span class="text-[10px] text-ink-600 rounded bg-ink-800/30 px-1.5 py-0.5">{entry.category}</span>
						</div>
					</div>
				{/each}
				{#if foreshadowing.entries.length === 0}
					<p class="text-center text-ink-500 py-8">尚无伏笔数据</p>
				{/if}
			</div>

		{:else if activeTab === 'macro' && macroAnalysis.length > 0}
			<div class="space-y-4">
				{#each macroAnalysis as window (window.id)}
					<div class="rounded-xl border border-ink-800/50 bg-ink-900/20 p-4 space-y-2">
						<div class="flex items-center justify-between">
							<h3 class="text-sm font-medium text-ink-200">第{window.start_chapter}–{window.end_chapter}章</h3>
						</div>
						{#if window.plot_arc}
							<p class="text-xs text-ink-400">{window.plot_arc}</p>
						{/if}
						{#if window.arc_summary}
							<p class="text-sm text-ink-300">{window.arc_summary}</p>
						{/if}
						{#if window.key_conflicts.length > 0}
							<div class="flex flex-wrap gap-1.5">
								{#each window.key_conflicts as conflict}
									<span class="rounded bg-red-500/10 text-red-400 border border-red-500/20 px-1.5 py-0.5 text-[10px]">{conflict}</span>
								{/each}
							</div>
						{/if}
					</div>
				{/each}
			</div>

		{:else if activeTab === 'summaries' && summaries.length > 0}
			<div class="space-y-2">
				{#each summaries as s (s.id)}
					<div class="rounded-lg border border-ink-800/50 bg-ink-900/20 p-3">
						<div class="flex items-center gap-2 mb-1">
							<span class="text-[10px] font-medium text-accent-400">第{s.chapter_index}章</span>
							{#if s.time_marker}
								<span class="text-[10px] text-ink-600">{s.time_marker}</span>
							{/if}
							{#if s.location}
								<span class="text-[10px] text-ink-600">@ {s.location}</span>
							{/if}
							{#if s.sentiment}
								<span class="text-[10px] rounded bg-ink-800/50 px-1 py-0.5 text-ink-500">{s.sentiment}</span>
							{/if}
						</div>
						<p class="text-xs text-ink-300">{s.summary}</p>
						{#if s.key_event}
							<p class="text-[10px] text-ink-500 mt-1">🔑 {s.key_event}</p>
						{/if}
					</div>
				{/each}
			</div>
		{/if}
	{/if}
</div>
