<script lang="ts">
	import AlertTriangleIcon from '@lucide/svelte/icons/triangle-alert';
	import ChevronDownIcon from '@lucide/svelte/icons/chevron-down';
	import { createQuery } from '@tanstack/svelte-query';

	import { Badge } from '$lib/components/ui/badge';
	import { Button } from '$lib/components/ui/button';
	import Skeleton from '$lib/components/ui/Skeleton.svelte';
	import * as m from '$lib/paraglide/messages.js';
	import { api } from '$services/api';
	import type { DuplicateDiffChange } from '$lib/types/models';
	import { formatLocaleNumber as formatNumber, getErrorMessage } from '$lib/utils';

	interface Props {
		pairId: string;
		matchId: string;
	}

	let { pairId, matchId }: Props = $props();
	let open = $state(false);

	const diff = createQuery(() => ({
		queryKey: ['duplicates', 'pair', pairId, 'match', matchId, 'diff'],
		queryFn: () => api.getDuplicateMatchDiff(pairId, matchId),
		enabled: open,
		staleTime: Infinity,
	}));

	function changeClass(change: DuplicateDiffChange): string {
		if (change.tag === 'insert') return 'bg-accent-500/10 text-accent-200';
		if (change.tag === 'delete') return 'bg-destructive/10 text-destructive line-through decoration-destructive/60';
		return 'text-ink-400';
	}

</script>

<details
	bind:open
	class="group mt-2 rounded-md bg-ink-950/45 ring-1 ring-ink-800/60"
>
	<summary class="flex cursor-pointer list-none items-center justify-between gap-2 px-3 py-2 text-xs font-medium text-ink-300 outline-none focus-visible:ring-2 focus-visible:ring-accent-500/50">
		<span>{open ? m.duplicates_diff_collapse() : m.duplicates_diff_expand()}</span>
		<ChevronDownIcon class="size-4 transition-transform group-open:rotate-180" />
	</summary>

	<div class="flex flex-col gap-3 border-t border-ink-800/60 p-3">
		{#if diff.isLoading}
			<Skeleton class="h-24 w-full" />
		{:else if diff.isError}
			<div class="flex items-start justify-between gap-3 text-xs text-destructive">
				<span class="flex items-start gap-1.5"><AlertTriangleIcon class="mt-0.5 size-3.5 shrink-0" />{m.duplicates_diff_load_failed({ error: getErrorMessage(diff.error) })}</span>
				<Button size="xs" variant="outline" onclick={() => diff.refetch()}>{m.common_retry()}</Button>
			</div>
		{:else if diff.data}
			<div class="flex flex-wrap items-center gap-2 text-[11px] text-ink-500">
				<Badge variant="outline">{m.duplicates_diff_similarity({ score: Math.round(diff.data.ratio * 100) })}</Badge>
				<span>{m.duplicates_character_count({ side: 'A', count: formatNumber(diff.data.chapter_a.character_count) })}</span>
				<span>{m.duplicates_character_count({ side: 'B', count: formatNumber(diff.data.chapter_b.character_count) })}</span>
				{#if diff.data.truncated}<span class="text-warning">{m.duplicates_diff_truncated()}</span>{/if}
			</div>
			<div class="max-h-64 overflow-y-auto rounded-md bg-ink-950 p-3 font-mono text-xs leading-6 ring-1 ring-ink-800/70">
				{#each diff.data.changes as change}
					<span class="whitespace-pre-wrap break-words px-0.5 {changeClass(change)}">
						{change.tag === 'insert' ? '+ ' : change.tag === 'delete' ? '− ' : ''}{change.value}
					</span>
				{/each}
			</div>
		{/if}
	</div>
</details>
