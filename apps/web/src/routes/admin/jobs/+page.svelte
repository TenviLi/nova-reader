<script lang="ts">
	import { createQuery, createMutation, useQueryClient } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { toast } from 'svelte-sonner';
	import { Clock, Play, Pause, RefreshCw, CheckCircle, AlertCircle, ChevronDown, ChevronRight } from 'lucide-svelte';

	const queryClient = useQueryClient();

	let expandedJobId = $state<string | null>(null);

	const jobs = createQuery(() => ({
		queryKey: ['admin', 'jobs'],
		queryFn: () => api.getScheduledJobs(),
		refetchInterval: 10_000,
	}));

	const toggleMutation = createMutation(() => ({
		mutationFn: (params: { jobId: string; enabled: boolean }) =>
			api.toggleJob(params.jobId, params.enabled),
		onSuccess: () => {
			queryClient.invalidateQueries({ queryKey: ['admin', 'jobs'] });
			toast.success('任务状态已更新');
		},
		onError: () => toast.error('操作失败'),
	}));

	function formatCron(cron: string): string {
		const presets: Record<string, string> = {
			'0 * * * *': '每小时',
			'0 0 * * *': '每天 00:00',
			'0 */6 * * *': '每 6 小时',
			'0 */12 * * *': '每 12 小时',
			'0 2 * * *': '每天 02:00',
			'*/30 * * * *': '每 30 分钟',
			'0 3 * * 0': '每周日 03:00',
		};
		return presets[cron] ?? cron;
	}

	function formatDuration(ms: number | null): string {
		if (ms === null) return '—';
		if (ms < 1000) return `${ms}ms`;
		if (ms < 60_000) return `${(ms / 1000).toFixed(1)}s`;
		return `${Math.floor(ms / 60_000)}m ${Math.floor((ms % 60_000) / 1000)}s`;
	}

	function timeAgo(dateStr: string | null): string {
		if (!dateStr) return '从未';
		const diff = Date.now() - new Date(dateStr).getTime();
		const mins = Math.floor(diff / 60_000);
		if (mins < 1) return '刚刚';
		if (mins < 60) return `${mins}分钟前`;
		const hours = Math.floor(mins / 60);
		if (hours < 24) return `${hours}小时前`;
		return `${Math.floor(hours / 24)}天前`;
	}
</script>

<svelte:head>
	<title>定时任务 — Nova Reader Admin</title>
</svelte:head>

<div class="mx-auto max-w-[1600px] px-4 py-6 sm:px-6 lg:px-8 space-y-6 animate-fade-in">
	<div class="flex items-center justify-between">
		<div>
			<h1 class="text-2xl font-bold text-ink-50">定时任务</h1>
			<p class="mt-1 text-sm text-ink-400">管理后台自动化任务的调度</p>
		</div>
		<button
			onclick={() => queryClient.invalidateQueries({ queryKey: ['admin', 'jobs'] })}
			class="flex items-center gap-2 px-3 py-1.5 text-sm text-ink-400 hover:text-ink-200 rounded-lg border border-ink-800 hover:border-ink-600 transition-colors"
		>
			<RefreshCw class="w-3.5 h-3.5" />
			刷新
		</button>
	</div>

	{#if jobs.isLoading}
		<div class="space-y-3">
			{#each Array(5) as _}
				<div class="h-16 rounded-xl bg-ink-900/50 animate-pulse"></div>
			{/each}
		</div>
	{:else if jobs.data}
		<div class="space-y-3">
			{#each jobs.data as job}
				<div class="rounded-xl border border-ink-800/50 bg-ink-900/60 hover:bg-ink-900/80 transition-colors overflow-hidden">
					<div
						class="flex items-center gap-4 p-4 cursor-pointer"
						role="button"
						tabindex="0"
						onclick={() => expandedJobId = expandedJobId === job.id ? null : job.id}
						onkeydown={(e) => {
							if (e.key === 'Enter' || e.key === ' ') {
								e.preventDefault();
								expandedJobId = expandedJobId === job.id ? null : job.id;
							}
						}}
					>
					<!-- Status indicator -->
					<div class="shrink-0">
						{#if job.status === 'active'}
							<CheckCircle class="w-5 h-5 text-emerald-400" />
						{:else}
							<Pause class="w-5 h-5 text-ink-500" />
						{/if}
					</div>

					<!-- Job info -->
					<div class="flex-1 min-w-0">
						<div class="flex items-center gap-2">
							<span class="text-sm font-medium text-ink-200">{job.name}</span>
							<span class="rounded-full px-2 py-0.5 text-[10px] font-mono bg-ink-800 text-ink-400">
								{formatCron(job.cron)}
							</span>
						</div>
						<div class="mt-1 flex items-center gap-4 text-xs text-ink-500">
							<span>上次: {timeAgo(job.last_run)}</span>
							<span>耗时: {formatDuration(job.last_duration_ms)}</span>
							<span>下次: {timeAgo(job.next_run)}</span>
						</div>
					</div>

					<!-- Expand indicator -->
					<div class="shrink-0 text-ink-500">
						{#if expandedJobId === job.id}
							<ChevronDown size={16} />
						{:else}
							<ChevronRight size={16} />
						{/if}
					</div>

					<!-- Toggle button -->
					<button
						onclick={(e) => { e.stopPropagation(); toggleMutation.mutate({ jobId: job.id, enabled: job.status !== 'active' }); }}
						disabled={toggleMutation.isPending}
						class="shrink-0 px-3 py-1.5 text-xs rounded-lg border transition-colors {job.status === 'active'
							? 'border-emerald-500/30 text-emerald-400 hover:bg-emerald-500/10'
							: 'border-ink-700 text-ink-400 hover:bg-ink-800'}"
					>
						{#if job.status === 'active'}
							<span class="flex items-center gap-1.5"><Pause class="w-3 h-3" /> 暂停</span>
						{:else}
							<span class="flex items-center gap-1.5"><Play class="w-3 h-3" /> 启用</span>
						{/if}
					</button>
					</div>

					<!-- Expandable log drawer -->
					{#if expandedJobId === job.id}
						<div class="border-t border-ink-800/50 p-4 bg-ink-950/50">
							<h4 class="text-xs font-medium text-ink-400 mb-2">最近执行日志</h4>
							{#if job.logs && job.logs.length > 0}
								<div class="space-y-1.5 max-h-48 overflow-y-auto font-mono text-xs">
									{#each job.logs as log}
										<div class="flex gap-3 text-ink-400">
											<span class="shrink-0 text-ink-600">{new Date(log.time).toLocaleTimeString()}</span>
											<span class:text-emerald-400={log.level === 'info'} class:text-amber-400={log.level === 'warn'} class:text-red-400={log.level === 'error'}>
												{log.message}
											</span>
										</div>
									{/each}
								</div>
							{:else}
								<p class="text-xs text-ink-600 italic">暂无日志记录</p>
							{/if}
						</div>
					{/if}
				</div>
			{/each}
		</div>
	{:else}
		<div class="text-center py-12 text-ink-500">
			<Clock class="w-12 h-12 mx-auto mb-3 opacity-30" />
			<p>暂无定时任务</p>
		</div>
	{/if}
</div>
