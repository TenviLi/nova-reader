<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { toast } from 'svelte-sonner';

	let loading = $state(true);
	let timeRange = $state(30);

	let summary = $state<{
		request_count: number;
		total_prompt_tokens: number;
		total_completion_tokens: number;
		total_tokens: number;
		total_cost_cents: number;
		avg_latency_ms: number;
		error_count: number;
		error_rate: number;
	} | null>(null);

	let dailyData = $state<Array<{
		date: string;
		requests: number;
		tokens: number;
		cost_cents: number;
	}>>([]);

	let maxDailyTokens = $derived(Math.max(...dailyData.map(d => d.tokens), 1));

	let operationData = $state<Array<{
		operation: string;
		count: number;
		tokens: number;
		cost_cents: number;
		avg_latency_ms: number;
	}>>([]);

	let recentLogs = $state<Array<{
		id: string;
		operation: string;
		model: string;
		provider: string;
		total_tokens: number;
		cost_cents: number;
		latency_ms: number;
		request_summary: string | null;
		success: boolean;
		error_message: string | null;
		username: string | null;
		book_title: string | null;
		created_at: string;
	}>>([]);

	onMount(() => loadData());

	async function loadData() {
		loading = true;
		try {
			const [s, d, o, recent] = await Promise.all([
				api.getAiUsageSummary(timeRange),
				api.getAiUsageDaily(timeRange),
				api.getAiUsageOperations(timeRange),
				api.getAiUsageRecent(timeRange, 50),
			]);
			summary = s;
			dailyData = d;
			operationData = o;
			recentLogs = recent;
		} catch (err) {
			const msg = err instanceof Error ? err.message : '加载失败';
			toast.error('AI 用量数据加载失败', { description: msg });
		} finally {
			loading = false;
		}
	}

	$effect(() => {
		timeRange;
		loadData();
	});

	function formatTokens(n: number): string {
		if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
		if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
		return n.toString();
	}

	function formatCost(cents: number): string {
		if (cents < 1) return `< ¢0.01`;
		if (cents >= 100) return `$${(cents / 100).toFixed(2)}`;
		return `¢${cents.toFixed(2)}`;
	}

	function formatDate(value: string): string {
		return new Date(value).toLocaleString('zh-CN', { month: '2-digit', day: '2-digit', hour: '2-digit', minute: '2-digit' });
	}

	const operationLabels: Record<string, string> = {
		chat: '对话',
		chat_stream: '流式对话',
		summarize: '摘要',
		extract_entities: '实体提取',
		analyze_style: '风格分析',
		suggest_tags: '标签推荐',
		generate_outline: '大纲生成',
		batch_process: '全书分析',
		translate: '翻译',
		translate_batch: '批量翻译',
		generate_entity_profile: '角色档案',
		ingest_embeddings: '向量化',
	};
</script>

<svelte:head>
	<title>Nova Reader — AI 用量统计</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 animate-fade-in">
	<!-- Header -->
	<div class="flex items-center justify-between mb-8">
		<div>
			<h1 class="text-2xl font-bold text-ink-100">AI 用量统计</h1>
			<p class="mt-1 text-sm text-ink-400">Token 消耗、费用估算、调用频次追踪</p>
		</div>
		<div class="flex gap-2">
			{#each [7, 30, 90] as days}
				<button
					onclick={() => timeRange = days}
					class="rounded-lg px-3 py-1.5 text-sm transition-colors"
					class:bg-accent-600={timeRange === days}
					class:text-white={timeRange === days}
					class:bg-ink-800={timeRange !== days}
					class:text-ink-300={timeRange !== days}
				>
					{days}天
				</button>
			{/each}
		</div>
	</div>

	{#if loading}
		<div class="flex items-center justify-center py-20">
			<div class="h-8 w-8 animate-spin rounded-full border-2 border-accent-500 border-t-transparent"></div>
		</div>
	{:else if summary}
		<!-- Summary Cards -->
		<div class="grid grid-cols-2 gap-4 md:grid-cols-4 mb-8">
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<p class="text-xs text-ink-400">总请求数</p>
				<p class="mt-1 text-2xl font-bold text-ink-100">{(summary.request_count ?? 0).toLocaleString()}</p>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<p class="text-xs text-ink-400">总 Token 消耗</p>
				<p class="mt-1 text-2xl font-bold text-ink-100">{formatTokens(summary.total_tokens ?? 0)}</p>
				<p class="mt-0.5 text-[10px] text-ink-500">
					输入 {formatTokens(summary.total_prompt_tokens ?? 0)} / 输出 {formatTokens(summary.total_completion_tokens ?? 0)}
				</p>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<p class="text-xs text-ink-400">估算费用</p>
				<p class="mt-1 text-2xl font-bold text-emerald-400">{formatCost(summary.total_cost_cents ?? 0)}</p>
			</div>
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<p class="text-xs text-ink-400">平均延迟</p>
				<p class="mt-1 text-2xl font-bold text-ink-100">{summary.avg_latency_ms ?? 0}ms</p>
				<p class="mt-0.5 text-[10px] text-ink-500">
					错误率 {((summary.error_rate ?? 0) * 100).toFixed(1)}% ({summary.error_count ?? 0} 次失败)
				</p>
			</div>
		</div>

		<!-- Daily Chart (simple bar visualization) -->
		{#if dailyData.length > 0}
			<div class="mb-8 rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<h3 class="mb-4 text-sm font-medium text-ink-200">每日用量趋势</h3>
				<div class="flex items-end gap-1 h-32">
					{#each dailyData.slice(-30) as day}
						<div class="group relative flex-1 min-w-1">
							<div
								class="w-full rounded-t bg-accent-500/70 hover:bg-accent-400 transition-colors"
								style="height: {Math.max(2, (day.tokens / maxDailyTokens) * 100)}%"
							></div>
							<div class="absolute bottom-full left-1/2 -translate-x-1/2 mb-2 hidden group-hover:block z-10 whitespace-nowrap rounded-md bg-ink-800 px-2 py-1 text-[10px] text-ink-200 shadow-lg">
								<div>{day.date}</div>
								<div>{formatTokens(day.tokens)} tokens</div>
								<div>{day.requests} 次调用</div>
							</div>
						</div>
					{/each}
				</div>
				<div class="mt-2 flex justify-between text-[10px] text-ink-500">
					<span>{dailyData[0]?.date ?? ''}</span>
					<span>{dailyData[dailyData.length - 1]?.date ?? ''}</span>
				</div>
			</div>
		{/if}

		<!-- Operation Breakdown -->
		{#if operationData.length > 0}
			<div class="mb-8 rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<h3 class="mb-4 text-sm font-medium text-ink-200">按操作类型统计</h3>
				<div class="overflow-x-auto">
					<table class="w-full text-sm">
						<thead>
							<tr class="text-left text-xs text-ink-400">
								<th class="pb-3">操作</th>
								<th class="pb-3 text-right">调用次数</th>
								<th class="pb-3 text-right">Token 消耗</th>
								<th class="pb-3 text-right">费用</th>
								<th class="pb-3 text-right">平均延迟</th>
							</tr>
						</thead>
						<tbody>
							{#each operationData as op}
								<tr class="border-t border-ink-800/30">
									<td class="py-2.5 text-ink-200">{operationLabels[op.operation] ?? op.operation}</td>
									<td class="py-2.5 text-right text-ink-300">{op.count}</td>
									<td class="py-2.5 text-right text-ink-300">{formatTokens(op.tokens)}</td>
									<td class="py-2.5 text-right text-emerald-400">{formatCost(op.cost_cents)}</td>
									<td class="py-2.5 text-right text-ink-400">{op.avg_latency_ms}ms</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</div>
		{/if}

		{#if recentLogs.length > 0}
			<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-4">
				<h3 class="mb-4 text-sm font-medium text-ink-200">最近调用记录</h3>
				<div class="overflow-x-auto">
					<table class="w-full text-sm">
						<thead>
							<tr class="text-left text-xs text-ink-400">
								<th class="pb-3">时间</th>
								<th class="pb-3">操作</th>
								<th class="pb-3">模型</th>
								<th class="pb-3">关联</th>
								<th class="pb-3 text-right">Token</th>
								<th class="pb-3 text-right">费用</th>
								<th class="pb-3 text-right">延迟</th>
								<th class="pb-3 text-right">状态</th>
							</tr>
						</thead>
						<tbody>
							{#each recentLogs as log}
								<tr class="border-t border-ink-800/30">
									<td class="py-2.5 pr-4 text-xs text-ink-500 whitespace-nowrap">{formatDate(log.created_at)}</td>
									<td class="py-2.5 pr-4 text-ink-200">{operationLabels[log.operation] ?? log.operation}</td>
									<td class="py-2.5 pr-4 text-xs text-ink-400">
										<div>{log.model}</div>
										<div class="text-ink-600">{log.provider}</div>
									</td>
									<td class="py-2.5 pr-4 text-xs text-ink-400 max-w-[240px]">
										<div class="truncate">{log.book_title ?? log.request_summary ?? '全局调用'}</div>
										{#if log.username}
											<div class="text-ink-600">@{log.username}</div>
										{/if}
									</td>
									<td class="py-2.5 text-right text-ink-300">{formatTokens(log.total_tokens)}</td>
									<td class="py-2.5 text-right text-emerald-400">{formatCost(log.cost_cents)}</td>
									<td class="py-2.5 text-right text-ink-400">{log.latency_ms}ms</td>
									<td class="py-2.5 text-right">
										<span class="rounded-full px-2 py-0.5 text-[10px] {log.success ? 'bg-emerald-500/10 text-emerald-400' : 'bg-red-500/10 text-red-400'}" title={log.error_message ?? undefined}>
											{log.success ? '成功' : '失败'}
										</span>
									</td>
								</tr>
							{/each}
						</tbody>
					</table>
				</div>
			</div>
		{/if}
	{:else}
		<div class="text-center py-20">
			<p class="text-ink-400">暂无 AI 调用数据</p>
			<p class="mt-1 text-sm text-ink-500">使用 AI 功能后，用量统计将在此展示</p>
		</div>
	{/if}
</div>
