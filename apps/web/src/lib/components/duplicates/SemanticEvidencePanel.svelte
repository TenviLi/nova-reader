<script lang="ts">
	import * as m from '$lib/paraglide/messages.js';
	import type { DuplicatePair } from '$lib/types/models';

	interface Props {
		pair: DuplicatePair;
	}

	let { pair }: Props = $props();

	let evidence = $derived(pair.evidence.semantic ?? null);
	let semanticScore = $derived(pair.semantic_score ?? evidence?.score ?? null);

	function formatPercent(value: number | null): string {
		return value === null ? '—' : `${Math.round(value * 100)}%`;
	}
</script>

<section class="rounded-lg bg-accent-950/20 p-4 ring-1 ring-accent-500/20" aria-label={m.duplicates_semantic_evidence_title()}>
	<div class="flex flex-wrap items-center justify-between gap-2">
		<h3 class="text-sm font-semibold text-ink-100">{m.duplicates_semantic_evidence_title()}</h3>
		{#if semanticScore !== null}
			<span class="text-sm font-semibold tabular-nums text-accent-300">
				{m.duplicates_semantic_score({ score: Math.round(semanticScore * 100) })}
			</span>
		{/if}
	</div>

	{#if evidence}
		<div class="mt-3 grid gap-2 text-xs text-ink-300 sm:grid-cols-2 xl:grid-cols-5">
			<span>{m.duplicates_semantic_independent_chapters({ count: evidence.independent_chapter_matches })}</span>
			<span>{m.duplicates_semantic_independent_chunks({ count: evidence.independent_chunk_matches })}</span>
			<span>{m.duplicates_semantic_ordered_pairs({ count: evidence.ordered_chapter_matches.length })}</span>
			<span>{m.duplicates_semantic_order_score({ score: Math.round(evidence.order_score * 100) })}</span>
			<span>{m.duplicates_semantic_sample_coverage({ a: formatPercent(evidence.sample_coverage_a), b: formatPercent(evidence.sample_coverage_b) })}</span>
		</div>

		{#if evidence.ordered_chapter_matches.length > 0}
			<ul class="mt-3 flex flex-wrap gap-2" aria-label={m.duplicates_semantic_ordered_pairs_list()}>
				{#each evidence.ordered_chapter_matches as match}
					<li class="rounded-md bg-ink-950/50 px-2.5 py-1.5 text-xs tabular-nums text-ink-300 ring-1 ring-ink-800/70">
						{m.duplicates_semantic_ordered_pair({
							chapterA: match.chapter_a_index + 1,
							chapterB: match.chapter_b_index + 1,
							score: Math.round(match.score * 100),
						})}
					</li>
				{/each}
			</ul>
		{/if}
	{:else}
		<p class="mt-3 text-xs leading-5 text-ink-500">{m.duplicates_semantic_evidence_unavailable()}</p>
	{/if}
</section>
