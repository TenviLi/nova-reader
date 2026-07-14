<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { Bell, CheckCircle, AlertCircle, Loader, Upload, Scan, Brain } from 'lucide-svelte';

	let { maxItems = 10 } = $props<{ maxItems?: number }>();

	const activities = createQuery(() => ({
		queryKey: ['activities', 'recent'],
		queryFn: () => api.getRecentActivities(maxItems),
		refetchInterval: 15_000, // Poll every 15s
	}));

	const iconMap: Record<string, typeof Bell> = {
		book_added: Upload,
		scan_completed: Scan,
		task_completed: CheckCircle,
		task_failed: AlertCircle,
		processing: Loader,
		entity_extracted: Brain,
	};

	function timeAgo(dateStr: string): string {
		const diff = Date.now() - new Date(dateStr).getTime();
		const mins = Math.floor(diff / 60000);
		if (mins < 1) return '刚刚';
		if (mins < 60) return `${mins}分钟前`;
		const hours = Math.floor(mins / 60);
		if (hours < 24) return `${hours}小时前`;
		return `${Math.floor(hours / 24)}天前`;
	}
</script>

<div class="rounded-xl border border-ink-800/50 bg-ink-900/80 p-5">
	<h3 class="text-sm font-semibold text-ink-200 flex items-center gap-2 mb-3">
		<Bell class="w-4 h-4 text-ink-400" />
		最近活动
	</h3>

	{#if activities.data && activities.data.length > 0}
		<div class="space-y-2">
			{#each activities.data as activity}
				{@const Icon = iconMap[activity.type] ?? Bell}
				<div class="flex items-start gap-3 py-2 border-b border-ink-800/20 last:border-0">
					<div class="shrink-0 mt-0.5">
						<Icon class="w-4 h-4 {activity.type === 'task_failed' ? 'text-red-400' : 'text-ink-500'}" />
					</div>
					<div class="flex-1 min-w-0">
						<p class="text-xs text-ink-300 truncate">{activity.message}</p>
						<p class="text-[10px] text-ink-600 mt-0.5">{timeAgo(activity.created_at)}</p>
					</div>
				</div>
			{/each}
		</div>
	{:else}
		<p class="text-xs text-ink-600 text-center py-4">暂无活动</p>
	{/if}
</div>
