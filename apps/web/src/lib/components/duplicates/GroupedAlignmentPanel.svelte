<script lang="ts">
	import ArrowRightIcon from '@lucide/svelte/icons/arrow-right';

	import * as m from '$lib/paraglide/messages.js';
	import type { DuplicateAlignmentGroupEvidence, DuplicatePair } from '$lib/types/models';
	import { formatLocaleNumber as formatNumber } from '$lib/utils';

	interface Props {
		pair: DuplicatePair;
	}

	let { pair }: Props = $props();
	let groups = $derived(
		(pair.evidence.chapter_boundary_groups ?? []).filter(
			(group) => group.mapping_shape !== 'one_to_one' || group.segment_count > 1,
		),
	);

	function chapterSet(indices: number[]): string {
		const chapters = [...indices].sort((left, right) => left - right).map((index) => index + 1);
		if (chapters.length === 0) return '—';
		const contiguous = chapters.every((chapter, index) => index === 0 || chapter === chapters[index - 1] + 1);
		if (contiguous && chapters.length > 1) {
			return m.duplicates_boundary_chapter_range({ start: chapters[0], end: chapters.at(-1) ?? chapters[0] });
		}
		return m.duplicates_boundary_chapter_list({ chapters: chapters.join(', ') });
	}

	function shapeLabel(shape: DuplicateAlignmentGroupEvidence['mapping_shape']): string {
		switch (shape) {
			case 'one_to_many': return m.duplicates_boundary_one_to_many();
			case 'many_to_one': return m.duplicates_boundary_many_to_one();
			case 'many_to_many': return m.duplicates_boundary_many_to_many();
			default: return m.duplicates_boundary_one_to_one();
		}
	}

</script>

{#if groups.length > 0}
	<section class="rounded-lg bg-accent-950/15 p-4 ring-1 ring-accent-500/20" aria-label={m.duplicates_boundary_title()}>
		<div>
			<h3 class="text-sm font-semibold text-ink-100">{m.duplicates_boundary_title()}</h3>
			<p class="mt-1 text-xs leading-5 text-ink-500">{m.duplicates_boundary_description()}</p>
		</div>

		<ul class="mt-3 flex flex-col gap-2">
			{#each groups as group (group.id)}
				<li class="grid grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] items-center gap-2 rounded-md bg-ink-950/45 p-3 ring-1 ring-ink-800/60">
					<div class="min-w-0">
						<p class="font-mono text-[10px] font-semibold uppercase tracking-[0.18em] text-ink-500">A</p>
						<p class="mt-1 truncate text-xs font-medium text-ink-200">{chapterSet(group.chapters_a)}</p>
					</div>

					<div class="flex min-w-24 flex-col items-center gap-1" aria-label={shapeLabel(group.mapping_shape)}>
						<div class="flex w-full items-center text-accent-400/80">
							<span class="h-px flex-1 border-t border-dashed border-accent-500/35"></span>
							<ArrowRightIcon class="size-3.5" aria-hidden="true" />
						</div>
						<span class="text-center text-[10px] tabular-nums text-ink-500">
							{m.duplicates_boundary_group_metrics({
								shape: shapeLabel(group.mapping_shape),
								characters: formatNumber(group.matched_characters),
								segments: formatNumber(group.segment_count),
							})}
						</span>
					</div>

					<div class="min-w-0 text-right">
						<p class="font-mono text-[10px] font-semibold uppercase tracking-[0.18em] text-ink-500">B</p>
						<p class="mt-1 truncate text-xs font-medium text-ink-200">{chapterSet(group.chapters_b)}</p>
					</div>
				</li>
			{/each}
		</ul>
	</section>
{/if}
