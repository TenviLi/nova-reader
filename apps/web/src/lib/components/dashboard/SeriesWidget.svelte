<script lang="ts">
	import { api } from '$services/api';
	import { onMount } from 'svelte';
	import { Layers, ArrowRight, BookOpen } from 'lucide-svelte';

	import type { Series } from '$types/models';

	let seriesList = $state<Series[]>([]);
	let loading = $state(true);

	onMount(async () => {
		try {
			const data = await api.getSeriesList();
			seriesList = (data ?? []).slice(0, 5);
		} catch {
			// graceful fallback
		} finally {
			loading = false;
		}
	});

	const statusColors: Record<string, string> = {
		ongoing: 'text-emerald-400',
		completed: 'text-blue-400',
		hiatus: 'text-amber-400',
		cancelled: 'text-red-400',
		unknown: 'text-ink-500',
	};
</script>

<div class="rounded-xl border border-ink-800/40 bg-ink-900/30 p-5">
	<div class="mb-4 flex items-center justify-between">
		<h3 class="text-sm font-semibold text-ink-200 flex items-center gap-2">
			<Layers size={16} class="text-blue-400" />
			系列
		</h3>
		<a href="/series" class="group inline-flex items-center gap-1 text-xs text-ink-400 hover:text-accent-400 transition-colors">
			管理
			<ArrowRight size={12} class="transition-transform group-hover:translate-x-0.5" />
		</a>
	</div>

	{#if loading}
		<div class="space-y-3">
			{#each Array(3) as _}
				<div class="h-10 rounded bg-ink-800/50 animate-pulse"></div>
			{/each}
		</div>
	{:else if seriesList.length === 0}
		<p class="text-sm text-ink-500 py-4 text-center">暂无系列</p>
	{:else}
		<div class="space-y-2">
			{#each seriesList as s (s.id)}
				<a href="/series/{s.id}" class="group flex items-center gap-3 rounded-lg p-2 -mx-2 hover:bg-ink-800/30 transition-colors">
					<div class="shrink-0 flex h-8 w-8 items-center justify-center rounded-lg bg-ink-800/60">
						<BookOpen size={14} class={statusColors[s.status] ?? 'text-ink-500'} />
					</div>
					<div class="flex-1 min-w-0">
						<p class="text-sm text-ink-200 truncate group-hover:text-accent-400 transition-colors">{s.name}</p>
						<p class="text-xs text-ink-500">{s.book_count} 本</p>
					</div>
				</a>
			{/each}
		</div>
	{/if}
</div>
