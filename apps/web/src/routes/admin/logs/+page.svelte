<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { ScrollText, AlertTriangle, Info, Bug, AlertCircle } from 'lucide-svelte';

	type LogLevel = 'all' | 'error' | 'warn' | 'info' | 'debug';

	const levels: Array<{ value: LogLevel; label: string }> = [
		{ value: 'all', label: '全部级别' },
		{ value: 'error', label: 'ERROR' },
		{ value: 'warn', label: 'WARN' },
		{ value: 'info', label: 'INFO' },
		{ value: 'debug', label: 'DEBUG' },
	];

	let levelFilter = $state<LogLevel>('all');
	let page = $state(0);
	const pageSize = 50;

	const logs = createQuery(() => ({
		queryKey: ['admin', 'logs', levelFilter, page],
		queryFn: () => api.getSystemLogs({
			level: levelFilter === 'all' ? undefined : levelFilter,
			limit: pageSize,
			offset: page * pageSize,
		}),
		refetchInterval: 5_000,
	}));

	const levelIcons = {
		info: Info,
		warn: AlertTriangle,
		error: AlertCircle,
		debug: Bug,
	} as const;

	const levelColors = {
		info: 'text-blue-400',
		warn: 'text-amber-400',
		error: 'text-red-400',
		debug: 'text-ink-500',
	} as const;

	function formatTimestamp(ts: string): string {
		return new Date(ts).toLocaleString('zh-CN', {
			hour: '2-digit',
			minute: '2-digit',
			second: '2-digit',
			month: '2-digit',
			day: '2-digit',
		});
	}
</script>

<svelte:head>
	<title>系统日志 — Nova Reader Admin</title>
</svelte:head>

<div class="space-y-6 animate-fade-in">
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">系统日志</h1>
			<p class="mt-1 text-sm text-ink-400">实时查看后端运行日志</p>
		</div>

		<label class="flex items-center gap-2 text-xs text-ink-400">
			<span>级别</span>
			<select
				bind:value={levelFilter}
				name="admin-log-level"
				aria-label="日志级别筛选"
				onchange={() => page = 0}
				class="rounded-lg border border-ink-800 bg-ink-900 px-3 py-1.5 text-xs text-ink-200 transition-colors hover:border-ink-700 focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-accent-400/70"
			>
				{#each levels as level}
					<option value={level.value}>{level.label}</option>
				{/each}
			</select>
		</label>
	</div>

	{#if logs.isLoading}
		<div class="space-y-1">
			{#each Array(10) as _}
				<div class="h-8 rounded bg-ink-900/50 animate-pulse"></div>
			{/each}
		</div>
	{:else if logs.data && logs.data.length > 0}
		<div class="rounded-xl border border-ink-800/50 overflow-hidden">
			<table class="w-full text-xs">
				<thead>
					<tr class="bg-ink-900/80 text-ink-400 text-left">
						<th class="px-3 py-2 w-8" aria-label="级别"></th>
						<th class="px-3 py-2 w-36">时间</th>
						<th class="px-3 py-2 w-40">模块</th>
						<th class="px-3 py-2">消息</th>
					</tr>
				</thead>
				<tbody class="divide-y divide-ink-800/30">
					{#each logs.data as log}
						{@const Icon = levelIcons[log.level] ?? Info}
						<tr class="hover:bg-ink-900/40 transition-colors">
							<td class="px-3 py-1.5">
								<Icon class="w-3.5 h-3.5 {levelColors[log.level] ?? 'text-ink-500'}" />
							</td>
							<td class="px-3 py-1.5 text-ink-500 font-mono">{formatTimestamp(log.timestamp)}</td>
							<td class="px-3 py-1.5 text-ink-400 font-mono truncate max-w-40">{log.target}</td>
							<td class="px-3 py-1.5 text-ink-300 truncate max-w-md" title={log.message}>{log.message}</td>
						</tr>
					{/each}
				</tbody>
			</table>
		</div>

		<!-- Pagination -->
		<div class="flex items-center justify-between text-xs text-ink-500">
			<button
				onclick={() => page = Math.max(0, page - 1)}
				disabled={page === 0}
				class="px-3 py-1 rounded border border-ink-800 disabled:opacity-30 hover:bg-ink-900 transition-colors"
			>
				上一页
			</button>
			<span>第 {page + 1} 页</span>
			<button
				onclick={() => page += 1}
				disabled={(logs.data?.length ?? 0) < pageSize}
				class="px-3 py-1 rounded border border-ink-800 disabled:opacity-30 hover:bg-ink-900 transition-colors"
			>
				下一页
			</button>
		</div>
	{:else}
		<div class="text-center py-12 text-ink-500">
			<ScrollText class="w-12 h-12 mx-auto mb-3 opacity-30" />
			<p>暂无日志</p>
		</div>
	{/if}
</div>
