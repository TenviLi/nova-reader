<script lang="ts">
	import { TrendingUp, Loader2, RefreshCw } from 'lucide-svelte';
	import { api } from '$lib/services/api';

	interface Props {
		bookId: string;
		bookTitle: string;
	}

	let { bookId, bookTitle }: Props = $props();

	interface SentimentPoint {
		chapter_index: number;
		chapter_title: string;
		score: number;
		emotion: string;
		key_event: string;
	}

	let arc = $state<SentimentPoint[]>([]);
	let loading = $state(false);
	let overallSentiment = $state(0);
	let hoveredPoint = $state<SentimentPoint | null>(null);

	const EMOTION_COLORS: Record<string, string> = {
		joy: '#f59e0b',
		hope: '#10b981',
		relief: '#06b6d4',
		surprise: '#8b5cf6',
		sadness: '#6b7280',
		anger: '#ef4444',
		fear: '#64748b',
		tension: '#f97316',
		neutral: '#94a3b8',
		unknown: '#d1d5db',
	};

	const EMOTION_LABELS: Record<string, string> = {
		joy: '喜悦', hope: '希望', relief: '释然', surprise: '惊讶',
		sadness: '悲伤', anger: '愤怒', fear: '恐惧', tension: '紧张',
		neutral: '平静', unknown: '未知',
	};

	async function analyze() {
		loading = true;
		try {
			const result = await api.post<{ arc: typeof arc; overall_sentiment: number }>('/ai/sentiment-arc', { book_id: bookId });
			arc = result.arc;
			overallSentiment = result.overall_sentiment;
		} catch {
			arc = [];
		} finally {
			loading = false;
		}
	}

	// SVG chart dimensions
	const WIDTH = 600;
	const HEIGHT = 200;
	const PADDING = 30;

	let pathData = $derived(() => {
		if (arc.length < 2) return '';
		const xStep = (WIDTH - 2 * PADDING) / (arc.length - 1);
		const yMid = HEIGHT / 2;
		const yScale = (HEIGHT - 2 * PADDING) / 2;

		return arc.map((point, i) => {
			const x = PADDING + i * xStep;
			const y = yMid - point.score * yScale;
			return `${i === 0 ? 'M' : 'L'} ${x} ${y}`;
		}).join(' ');
	});

	let pointPositions = $derived(() => {
		if (arc.length < 2) return [];
		const xStep = (WIDTH - 2 * PADDING) / (arc.length - 1);
		const yMid = HEIGHT / 2;
		const yScale = (HEIGHT - 2 * PADDING) / 2;

		return arc.map((point, i) => ({
			x: PADDING + i * xStep,
			y: yMid - point.score * yScale,
			point,
		}));
	});
</script>

<div class="rounded-lg border border-ink-200 bg-white p-4 dark:border-ink-700 dark:bg-ink-900">
	<!-- Header -->
	<div class="mb-4 flex items-center justify-between">
		<div class="flex items-center gap-2">
			<TrendingUp class="h-4 w-4 text-accent-500" />
			<h3 class="text-sm font-medium text-ink-800 dark:text-ink-200">情感曲线</h3>
			{#if overallSentiment !== 0}
				<span class="rounded bg-ink-100 px-1.5 py-0.5 text-xs dark:bg-ink-800"
					class:text-green-600={overallSentiment > 0.2}
					class:text-red-600={overallSentiment < -0.2}
					class:text-ink-500={Math.abs(overallSentiment) <= 0.2}
				>
					整体: {overallSentiment > 0 ? '+' : ''}{overallSentiment.toFixed(2)}
				</span>
			{/if}
		</div>
		<button
			class="flex items-center gap-1 rounded px-2 py-1 text-xs text-ink-500 hover:bg-ink-100 disabled:opacity-50 dark:hover:bg-ink-700"
			onclick={analyze}
			disabled={loading}
		>
			{#if loading}
				<Loader2 class="h-3 w-3 animate-spin" />
			{:else}
				<RefreshCw class="h-3 w-3" />
			{/if}
			{arc.length > 0 ? '重新分析' : '开始分析'}
		</button>
	</div>

	{#if loading}
		<div class="flex h-48 items-center justify-center">
			<div class="flex items-center gap-2 text-sm text-ink-400">
				<Loader2 class="h-4 w-4 animate-spin" />
				正在分析情感走向...
			</div>
		</div>
	{:else if arc.length === 0}
		<div class="flex h-48 items-center justify-center text-sm text-ink-400">
			点击"开始分析"生成情感曲线
		</div>
	{:else}
		<!-- SVG Chart -->
		<div class="relative">
			<svg viewBox="0 0 {WIDTH} {HEIGHT}" class="w-full" style="max-height: 200px;">
				<!-- Grid lines -->
				<line x1={PADDING} y1={HEIGHT/2} x2={WIDTH-PADDING} y2={HEIGHT/2}
					stroke="currentColor" stroke-opacity="0.1" stroke-dasharray="4" />
				<line x1={PADDING} y1={PADDING} x2={WIDTH-PADDING} y2={PADDING}
					stroke="currentColor" stroke-opacity="0.05" />
				<line x1={PADDING} y1={HEIGHT-PADDING} x2={WIDTH-PADDING} y2={HEIGHT-PADDING}
					stroke="currentColor" stroke-opacity="0.05" />

				<!-- Y-axis labels -->
				<text x="5" y={PADDING + 4} class="fill-current text-[9px] opacity-40">积极</text>
				<text x="5" y={HEIGHT - PADDING + 4} class="fill-current text-[9px] opacity-40">消极</text>

				<!-- Gradient fill under curve -->
				{#if pathData()}
					<defs>
						<linearGradient id="sentimentGradient" x1="0" y1="0" x2="0" y2="1">
							<stop offset="0%" stop-color="rgb(16, 185, 129)" stop-opacity="0.3" />
							<stop offset="50%" stop-color="rgb(148, 163, 184)" stop-opacity="0.05" />
							<stop offset="100%" stop-color="rgb(239, 68, 68)" stop-opacity="0.3" />
						</linearGradient>
					</defs>

					<!-- The sentiment curve -->
					<path d={pathData()} fill="none" stroke="rgb(245, 158, 11)" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" />

					<!-- Data points -->
					{#each pointPositions() as { x, y, point }}
						<circle
							cx={x} cy={y} r="4"
							fill={EMOTION_COLORS[point.emotion] ?? EMOTION_COLORS.neutral}
							stroke="white" stroke-width="1.5"
							role="img"
							aria-label="第{point.chapter_index + 1}章情绪：{EMOTION_LABELS[point.emotion] ?? point.emotion}"
							class="cursor-pointer transition-all hover:r-6"
							onmouseenter={() => hoveredPoint = point}
							onmouseleave={() => hoveredPoint = null}
						/>
					{/each}
				{/if}
			</svg>

			<!-- Tooltip -->
			{#if hoveredPoint}
				<div class="absolute left-1/2 top-0 z-10 -translate-x-1/2 rounded-lg border border-ink-200 bg-white p-2 shadow-lg dark:border-ink-600 dark:bg-ink-800">
					<p class="text-xs font-medium text-ink-800 dark:text-ink-200">
						第{hoveredPoint.chapter_index + 1}章: {hoveredPoint.chapter_title}
					</p>
					<p class="text-xs text-ink-500">
						{EMOTION_LABELS[hoveredPoint.emotion] ?? hoveredPoint.emotion}
						({hoveredPoint.score > 0 ? '+' : ''}{hoveredPoint.score.toFixed(2)})
					</p>
					<p class="text-xs text-ink-400">{hoveredPoint.key_event}</p>
				</div>
			{/if}
		</div>

		<!-- Legend -->
		<div class="mt-3 flex flex-wrap gap-2">
			{#each Object.entries(EMOTION_LABELS) as [key, label]}
				{#if arc.some(p => p.emotion === key)}
					<span class="flex items-center gap-1 text-xs text-ink-500">
						<span class="h-2 w-2 rounded-full" style="background: {EMOTION_COLORS[key]}"></span>
						{label}
					</span>
				{/if}
			{/each}
		</div>
	{/if}
</div>
