<script lang="ts">
	import * as m from '$lib/paraglide/messages.js';
	import type { DuplicatePair, DuplicatePrimaryVersionEvidence } from '$lib/types/models';
	import { formatLocaleNumber as formatNumber } from '$lib/utils';

	interface Props {
		pair: DuplicatePair;
	}

	let { pair }: Props = $props();
	let evidence = $derived(pair.evidence.primary_recommendation ?? null);
	let recommendedSide = $derived(
		pair.recommended_primary_id === pair.book_a.id
			? 'A'
			: pair.recommended_primary_id === pair.book_b.id
				? 'B'
				: null,
	);

	function integrity(value: DuplicatePrimaryVersionEvidence): string {
		return `${Math.round(value.text_integrity_score * 100)}%`;
	}
</script>

{#if evidence}
	<section class="rounded-lg bg-ink-950/35 p-4 ring-1 ring-ink-800/70" aria-label={m.duplicates_primary_evidence_title()}>
		<div class="flex flex-wrap items-center justify-between gap-2">
			<h3 class="text-sm font-semibold text-ink-100">{m.duplicates_primary_evidence_title()}</h3>
			{#if recommendedSide}
				<span class="text-xs font-medium text-accent-300">{m.duplicates_primary_evidence_recommended({ side: recommendedSide })}</span>
			{/if}
		</div>
		<p class="mt-1 text-xs leading-5 text-ink-500">{m.duplicates_primary_evidence_description()}</p>
		<div class="mt-3 grid gap-3 sm:grid-cols-2">
			{#each [['A', evidence.book_a], ['B', evidence.book_b]] as item}
				{@const [side, version] = item as ['A' | 'B', DuplicatePrimaryVersionEvidence]}
				<div class="rounded-md bg-ink-950/55 p-3 ring-1 ring-ink-800/60">
					<p class="text-xs font-semibold text-ink-300">{m.duplicates_version_label({ side })}</p>
					<p class="mt-2 text-xs leading-5 text-ink-500">
						{m.duplicates_primary_evidence_metrics({
							uniqueChapters: formatNumber(version.unique_informative_chapters),
							uniqueChars: formatNumber(version.unique_informative_chars),
							repeated: formatNumber(version.repeated_informative_chapters),
							integrity: integrity(version),
						})}
					</p>
				</div>
			{/each}
		</div>
	</section>
{/if}
