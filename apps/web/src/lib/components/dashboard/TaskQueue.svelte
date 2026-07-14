<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { api } from '$services/api';

	import type { Task } from '$types/models';

	const tasksQuery = createQuery(() => ({
		queryKey: ['tasks', 'dashboard'],
		queryFn: () => api.getTasks({ limit: 10 }),
		refetchInterval: 5_000,
	}));

	let tasks: Task[] = $derived((tasksQuery.data ?? []) as Task[]);
	let stats = $derived({
		queued: tasks.filter(t => t.status === 'queued').length,
		running: tasks.filter(t => t.status === 'running').length,
		failed: tasks.filter(t => t.status === 'failed').length,
	});
</script>

<div class="rounded-xl border border-ink-800/50 bg-ink-900/30 p-5">
	<div class="mb-4 flex items-center justify-between">
		<h3 class="text-sm font-semibold text-ink-100">任务队列</h3>
		<a href="/tasks" class="text-xs text-ink-400 hover:text-accent-400 transition-colors">
			管理 →
		</a>
	</div>

	<!-- Quick Stats -->
	<div class="mb-4 grid grid-cols-3 gap-2">
		<div class="rounded-lg bg-ink-800/50 p-2 text-center">
			<div class="text-lg font-bold text-ink-100">{stats.queued}</div>
			<div class="text-[10px] text-ink-500">排队中</div>
		</div>
		<div class="rounded-lg bg-ink-800/50 p-2 text-center">
			<div class="text-lg font-bold text-accent-400">{stats.running}</div>
			<div class="text-[10px] text-ink-500">执行中</div>
		</div>
		<div class="rounded-lg bg-ink-800/50 p-2 text-center">
			<div class="text-lg font-bold text-error">{stats.failed}</div>
			<div class="text-[10px] text-ink-500">失败</div>
		</div>
	</div>

	<!-- Task List -->
	{#if tasks.length === 0}
		<div class="py-4 text-center text-sm text-ink-500">
			暂无运行中的任务
		</div>
	{:else}
		<div class="space-y-2">
			{#each tasks.slice(0, 5) as task}
				<div class="flex items-center gap-3 rounded-lg bg-ink-800/30 px-3 py-2">
					<!-- Status indicator -->
					<div
						class="h-2 w-2 shrink-0 rounded-full"
						class:bg-accent-400={task.status === 'running'}
						class:bg-ink-500={task.status === 'queued'}
						class:bg-error={task.status === 'failed'}
						class:animate-pulse={task.status === 'running'}
					></div>

					<!-- Task info -->
					<div class="flex-1 overflow-hidden">
						<div class="truncate text-xs text-ink-200">{task.kind}</div>
						<div class="truncate text-[10px] text-ink-500">{task.id}</div>
					</div>
				</div>
			{/each}
		</div>
	{/if}
</div>
