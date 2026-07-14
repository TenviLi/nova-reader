<script lang="ts">
	import { api } from '$services/api';
	import { onMount, onDestroy } from 'svelte';
	import { toast } from 'svelte-sonner';
	import { ChevronDown, ChevronRight, RotateCcw, XCircle, Clock, Loader2 } from 'lucide-svelte';

	let tasks = $state.raw<Array<{
		id: string;
		kind: string;
		status: string;
		priority: string;
		progress: number;
		progress_message: string | null;
		error_message: string | null;
		book_id: string | null;
		category: string;
		retry_count: number;
		max_retries: number;
		payload: Record<string, unknown>;
		result: Record<string, unknown> | null;
		created_at: string;
		started_at: string | null;
		completed_at: string | null;
	}>>([]);

	let filterStatus = $state<string | null>(null);
	let filterCategory = $state<string | null>(null);
	let expandedTaskId = $state<string | null>(null);
	let stats = $state({ queued: 0, running: 0, completed_today: 0, failed_today: 0, dead_letter_count: 0, avg_processing_time_ms: 0 });
	let categoryStats = $state<Array<{ category: string; queued: number; running: number; completed_today: number }>>([]);
	let total = $state(0);
	let page = $state(1);
	let loading = $state(true);
	let pollInterval: ReturnType<typeof setInterval>;

	const statusColors: Record<string, string> = {
		queued: 'bg-ink-500',
		running: 'bg-accent-400 animate-pulse',
		completed: 'bg-emerald-400',
		failed: 'bg-red-400',
		cancelled: 'bg-ink-600',
		dead_letter: 'bg-amber-400',
		retrying: 'bg-yellow-400 animate-pulse',
	};

	const statusLabels: Record<string, string> = {
		queued: '排队中',
		running: '执行中',
		completed: '已完成',
		failed: '失败',
		cancelled: '已取消',
		dead_letter: '死信',
		retrying: '重试中',
	};

	const categoryLabels: Record<string, string> = {
		import: '导入',
		preprocess: '预处理',
		ai: 'AI 分析',
		index: '索引',
		maintenance: '维护',
	};

	const kindLabels: Record<string, string> = {
		clean_content: '清洗内容',
		generate_embeddings: '生成向量',
		extract_entities: '实体提取',
		index_meilisearch: '搜索索引',
		sync_neo4j: '图谱同步',
		compute_book_embedding: '书籍向量',
		generate_metadata: '生成元数据',
		detect_communities: '社区检测',
		parse_file: '解析文件',
		library_scan: '扫描书库',
		reindex_library: '重建书库索引',
		cleanup_orphan_covers: '清理孤儿封面',
		recompute_file_hashes: '重新计算哈希',
	};

	onMount(async () => {
		await loadAll();
		pollInterval = setInterval(loadAll, 3000);
	});

	onDestroy(() => {
		if (pollInterval) clearInterval(pollInterval);
	});

	async function loadAll() {
		await Promise.all([loadTasks(), loadStats()]);
		loading = false;
	}

	async function loadTasks() {
		try {
			const params: { status?: string; category?: string; page?: number; per_page?: number } = { page, per_page: 50 };
			if (filterStatus) params.status = filterStatus;
			if (filterCategory) params.category = filterCategory;
			const result = await api.getTaskQueue(params);
			tasks = result.data;
			total = result.total;
		} catch (err) {
			console.error('[Tasks] Failed to load:', err);
			tasks = [];
		}
	}

	async function loadStats() {
		try {
			const result = await api.getTaskStats();
			stats = result.stats;
			categoryStats = result.categories;
		} catch (err) {
			console.error('[Tasks] Failed to load stats:', err);
		}
	}

	async function retryTask(taskId: string) {
		try {
			await api.retryTask(taskId);
			toast.success('任务已重新排队');
			await loadAll();
		} catch {
			toast.error('重试失败');
		}
	}

	async function cancelTask(taskId: string) {
		try {
			await api.cancelTask(taskId);
			toast.success('任务已取消');
			await loadAll();
		} catch {
			toast.error('取消失败');
		}
	}

	function toggleTask(taskId: string) {
		expandedTaskId = expandedTaskId === taskId ? null : taskId;
	}

	function handleTaskRowKeydown(event: KeyboardEvent, taskId: string) {
		if (event.key === 'Enter' || event.key === ' ') {
			event.preventDefault();
			toggleTask(taskId);
		}
	}

	function formatTime(dateStr: string | null): string {
		if (!dateStr) return '—';
		const d = new Date(dateStr);
		return d.toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit', second: '2-digit' });
	}

	function durationBetween(start: string | null, end: string | null): string {
		if (!start) return '—';
		const s = new Date(start).getTime();
		const e = end ? new Date(end).getTime() : Date.now();
		const ms = e - s;
		if (ms < 1000) return `${ms}ms`;
		if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
		return `${Math.floor(ms / 60_000)}m ${Math.floor((ms % 60_000) / 1000)}s`;
	}
</script>

<svelte:head>
	<title>Nova Reader — 任务队列</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<!-- Header -->
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">任务队列</h1>
			<p class="mt-0.5 text-sm text-ink-400">后台处理进度 · 执行日志</p>
		</div>
		<div class="flex items-center gap-2">
			<span class="flex items-center gap-1.5 text-xs text-ink-400">
				<span class="h-2 w-2 rounded-full bg-emerald-400 animate-pulse"></span>
				实时更新
			</span>
		</div>
	</div>

	<!-- Stats bar -->
	<div class="grid grid-cols-5 gap-3">
		{#each [
			{ key: 'queued', label: '排队', value: stats.queued, color: 'text-ink-300' },
			{ key: 'running', label: '执行中', value: stats.running, color: 'text-accent-400' },
			{ key: 'completed', label: '已完成', value: stats.completed_today, color: 'text-emerald-400' },
			{ key: 'failed', label: '失败', value: stats.failed_today, color: 'text-red-400' },
			{ key: 'dead_letter', label: '死信', value: stats.dead_letter_count, color: 'text-amber-400' },
		] as stat}
			<button
				onclick={() => { filterStatus = filterStatus === stat.key ? null : stat.key; page = 1; loadAll(); }}
				class="rounded-xl border bg-ink-900/30 p-3 text-center transition-all {filterStatus === stat.key ? 'border-accent-500/30 bg-accent-500/5' : 'border-ink-800/50 hover:border-ink-700/50'}"
			>
				<div class="text-2xl font-bold tabular-nums {stat.color}">{stat.value}</div>
				<div class="text-xs text-ink-500">{stat.label}</div>
			</button>
		{/each}
	</div>

	<!-- Category filter -->
	<div class="flex items-center gap-2">
		<span class="text-xs text-ink-500">分类:</span>
		<button
			onclick={() => { filterCategory = null; page = 1; loadAll(); }}
			class="rounded-lg px-2.5 py-1 text-xs transition-all {!filterCategory ? 'bg-accent-500/20 text-accent-300' : 'text-ink-400 hover:text-ink-200'}"
		>全部</button>
		{#each ['import', 'preprocess', 'ai', 'index'] as cat}
			<button
				onclick={() => { filterCategory = filterCategory === cat ? null : cat; page = 1; loadAll(); }}
				class="rounded-lg px-2.5 py-1 text-xs transition-all {filterCategory === cat ? 'bg-accent-500/20 text-accent-300' : 'text-ink-400 hover:text-ink-200'}"
			>{categoryLabels[cat] ?? cat}</button>
		{/each}
		{#if stats.avg_processing_time_ms > 0}
			<span class="ml-auto text-xs text-ink-500">平均耗时: {(stats.avg_processing_time_ms / 1000).toFixed(1)}s</span>
		{/if}
	</div>

	<!-- Task list -->
	<div class="space-y-2">
		{#each tasks as task (task.id)}
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/20 overflow-hidden transition-all hover:border-ink-700/50">
				<!-- Task row -->
				<div
					role="button"
					tabindex="0"
					onclick={() => toggleTask(task.id)}
					onkeydown={(event) => handleTaskRowKeydown(event, task.id)}
					class="flex w-full items-center gap-4 px-4 py-3 text-left"
				>
					<!-- Expand arrow -->
					<span class="text-ink-500 transition-transform" class:rotate-90={expandedTaskId === task.id}>
						<ChevronRight size={14} />
					</span>

					<!-- Status dot -->
					<span class="h-2.5 w-2.5 shrink-0 rounded-full {statusColors[task.status] ?? 'bg-ink-600'}"></span>

					<!-- Kind -->
					<span class="min-w-[120px] text-sm font-medium text-ink-200">{kindLabels[task.kind] ?? task.kind}</span>

					<!-- Category badge -->
					<span class="rounded-md bg-ink-800/50 px-1.5 py-0.5 text-[10px] text-ink-400">{categoryLabels[task.category] ?? task.category}</span>

					<!-- Progress -->
					<div class="flex-1">
						{#if task.status === 'running'}
							<div class="flex items-center gap-2">
								<div class="h-1.5 flex-1 max-w-[200px] overflow-hidden rounded-full bg-ink-800">
									<div class="h-full rounded-full bg-accent-500 transition-all duration-500" style="width: {task.progress}%"></div>
								</div>
								<span class="text-xs text-ink-400 tabular-nums">{task.progress}%</span>
								{#if task.progress_message}
									<span class="text-xs text-ink-500 truncate max-w-[150px]">{task.progress_message}</span>
								{/if}
							</div>
						{:else if task.status === 'failed'}
							<span class="text-xs text-red-400/80 truncate block max-w-[300px]">{task.error_message ?? '未知错误'}</span>
						{:else if task.status === 'completed'}
							<span class="text-xs text-emerald-400/80">✓ 完成</span>
						{:else}
							<span class="text-xs text-ink-500">{statusLabels[task.status] ?? task.status}</span>
						{/if}
					</div>

					<!-- Duration -->
					<span class="text-xs text-ink-500 tabular-nums min-w-[60px] text-right">
						{#if task.status === 'running'}
							<span class="inline-flex items-center gap-1">
								<Loader2 size={10} class="animate-spin" />
								{durationBetween(task.started_at, null)}
							</span>
						{:else if task.started_at}
							{durationBetween(task.started_at, task.completed_at)}
						{:else}
							—
						{/if}
					</span>

					<!-- Time -->
					<span class="text-xs text-ink-500 min-w-[100px] text-right">{formatTime(task.created_at)}</span>

					<!-- Actions -->
					<div class="flex gap-1 min-w-[60px] justify-end">
						{#if task.status === 'running' || task.status === 'queued'}
							<button onclick={(event) => { event.stopPropagation(); cancelTask(task.id); }} class="rounded p-1 text-ink-400 hover:bg-red-500/10 hover:text-red-400 transition-colors" title="取消">
								<XCircle size={14} />
							</button>
						{/if}
						{#if task.status === 'failed' || task.status === 'dead_letter' || task.status === 'cancelled'}
							<button onclick={(event) => { event.stopPropagation(); retryTask(task.id); }} class="rounded p-1 text-ink-400 hover:bg-accent-500/10 hover:text-accent-400 transition-colors" title="重试">
								<RotateCcw size={14} />
							</button>
						{/if}
					</div>
				</div>

				<!-- Expanded detail -->
				{#if expandedTaskId === task.id}
					<div class="border-t border-ink-800/30 bg-ink-950/50 px-4 py-3">
						<div class="grid grid-cols-2 gap-4 text-xs mb-3">
							<div>
								<span class="text-ink-500">任务 ID</span>
								<code class="ml-2 text-ink-300 font-mono text-[11px]">{task.id}</code>
							</div>
							<div>
								<span class="text-ink-500">优先级</span>
								<span class="ml-2 text-ink-300">{task.priority}</span>
							</div>
							<div>
								<span class="text-ink-500">分类</span>
								<span class="ml-2 text-ink-300">{categoryLabels[task.category] ?? task.category}</span>
							</div>
							<div>
								<span class="text-ink-500">重试</span>
								<span class="ml-2 text-ink-300">{task.retry_count} / {task.max_retries}</span>
							</div>
							<div>
								<span class="text-ink-500">创建时间</span>
								<span class="ml-2 text-ink-300">{formatTime(task.created_at)}</span>
							</div>
							<div>
								<span class="text-ink-500">开始时间</span>
								<span class="ml-2 text-ink-300">{formatTime(task.started_at)}</span>
							</div>
							<div>
								<span class="text-ink-500">完成时间</span>
								<span class="ml-2 text-ink-300">{formatTime(task.completed_at)}</span>
							</div>
							<div>
								<span class="text-ink-500">耗时</span>
								<span class="ml-2 text-ink-300">{durationBetween(task.started_at, task.completed_at)}</span>
							</div>
						</div>

						<!-- Payload -->
						{#if task.payload && Object.keys(task.payload).length > 0}
							<div class="mt-3 pt-3 border-t border-ink-800/30">
								<div class="text-xs text-ink-500 mb-2">执行参数</div>
								<div class="rounded-lg bg-ink-900/80 p-3 font-mono text-xs text-ink-300 space-y-1 max-h-[200px] overflow-y-auto">
									{#each Object.entries(task.payload) as [key, value]}
										<div>
											<span class="text-ink-500">{key}:</span>
											<span class="ml-1">{typeof value === 'object' ? JSON.stringify(value) : String(value)}</span>
										</div>
									{/each}
								</div>
							</div>
						{/if}

						<!-- Result -->
						{#if task.result && Object.keys(task.result).length > 0}
							<div class="mt-3 pt-3 border-t border-ink-800/30">
								<div class="text-xs text-emerald-400/80 mb-2">执行结果</div>
								<pre class="rounded-lg bg-emerald-950/20 border border-emerald-900/20 p-3 font-mono text-xs text-emerald-300/80 whitespace-pre-wrap max-h-[200px] overflow-y-auto">{JSON.stringify(task.result, null, 2)}</pre>
							</div>
						{/if}

						<!-- Error output -->
						{#if task.error_message}
							<div class="mt-3 pt-3 border-t border-ink-800/30">
								<div class="text-xs text-red-400/80 mb-2">错误输出</div>
								<pre class="rounded-lg bg-red-950/30 border border-red-900/30 p-3 font-mono text-xs text-red-300/80 whitespace-pre-wrap max-h-[200px] overflow-y-auto">{task.error_message}</pre>
							</div>
						{/if}
					</div>
				{/if}
			</div>
		{/each}

		{#if !loading && tasks.length === 0}
			<div class="rounded-xl border border-dashed border-ink-800/50 py-16 text-center">
				<Clock class="mx-auto mb-3 h-8 w-8 text-ink-600" strokeWidth={1.5} />
				<p class="text-sm text-ink-500">
					{filterStatus || filterCategory ? '该筛选条件下暂无任务' : '暂无任务记录'}
				</p>
			</div>
		{/if}

		<!-- Pagination -->
		{#if total > 50}
			<div class="flex items-center justify-center gap-2 pt-4">
				<button
					disabled={page <= 1}
					onclick={() => { page--; loadAll(); }}
					class="rounded-lg px-3 py-1.5 text-xs text-ink-400 hover:text-ink-200 disabled:opacity-30"
				>上一页</button>
				<span class="text-xs text-ink-500">{page} / {Math.ceil(total / 50)}</span>
				<button
					disabled={page >= Math.ceil(total / 50)}
					onclick={() => { page++; loadAll(); }}
					class="rounded-lg px-3 py-1.5 text-xs text-ink-400 hover:text-ink-200 disabled:opacity-30"
				>下一页</button>
			</div>
		{/if}
	</div>
</div>
