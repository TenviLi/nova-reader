<script lang="ts">
	import { createQuery } from '@tanstack/svelte-query';
	import { api } from '$services/api';
	import { Flame, Trophy } from 'lucide-svelte';

	const stats = createQuery(() => ({
		queryKey: ['reading-stats', 'year'],
		queryFn: () => api.getReadingStats('year'),
		staleTime: 1000 * 60 * 10,
	}));

	let currentStreak = $derived(stats.data?.currentStreak ?? 0);
	let longestStreak = $derived(stats.data?.longestStreak ?? 0);
	let isOnFire = $derived(currentStreak >= 7);
	let nextMilestone = $derived(currentStreak < 7 ? 7 : currentStreak < 30 ? 30 : currentStreak < 100 ? 100 : 365);
	let progressPct = $derived(Math.min(100, (currentStreak / nextMilestone) * 100));
</script>

{#if stats.data}
	<div class="rounded-xl border border-ink-800/50 bg-ink-900/50 p-4">
		<div class="flex items-center justify-between mb-3">
			<h3 class="text-xs font-medium text-ink-400">阅读连续天数</h3>
			{#if isOnFire}
				<span class="inline-flex items-center gap-1 text-[10px] px-1.5 py-0.5 rounded-full bg-orange-500/10 text-orange-400"><Flame size={10} /> 连续阅读中</span>
			{/if}
		</div>

		<div class="flex items-end gap-4">
			<!-- Current streak -->
			<div class="flex items-center gap-2">
				<div class="flex h-10 w-10 items-center justify-center rounded-lg {isOnFire ? 'bg-orange-500/10' : 'bg-ink-800'}">
					<Flame class="w-5 h-5 {isOnFire ? 'text-orange-400' : 'text-ink-500'}" />
				</div>
				<div>
					<div class="text-2xl font-bold text-ink-50">{currentStreak}</div>
					<div class="text-[10px] text-ink-500">当前连续</div>
				</div>
			</div>

			<!-- Longest streak -->
			<div class="flex items-center gap-2 ml-auto">
				<Trophy class="w-4 h-4 text-amber-500/60" />
				<div class="text-right">
					<div class="text-sm font-medium text-ink-300">{longestStreak}</div>
					<div class="text-[10px] text-ink-500">最长记录</div>
				</div>
			</div>
		</div>

		<!-- Streak progress bar (toward next milestone) -->
		<div class="mt-3">
			<div class="flex items-center justify-between text-[10px] text-ink-500 mb-1">
				<span>{currentStreak} 天</span>
				<span>{nextMilestone} 天目标</span>
			</div>
			<div class="h-1.5 rounded-full bg-ink-800 overflow-hidden">
				<div
					class="h-full rounded-full transition-all duration-500 {isOnFire ? 'bg-gradient-to-r from-orange-500 to-amber-400' : 'bg-accent-500'}"
					style="width: {progressPct}%"
				></div>
			</div>
		</div>
	</div>
{/if}
